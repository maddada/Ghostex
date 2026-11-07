//! A working agent session that has gone quiet: no new transcript row and no change on its screen
//! for a long stretch. One `StallWatch` per working session keeps its output clock. The coordinator
//! supervisor (server/coordinator_runtime.rs) owns its threads' watches: at 8 minutes it nudges the
//! thread and reports to its coordinator, it reports again every 8 minutes until the thread moves,
//! and 8 minutes after the nudge the thread counts as stuck. The sidebar-wide watch
//! (server/session_stall_runtime.rs) owns every other agent session, which counts as stuck at 8
//! minutes. A stuck session draws the red dot (presentation `stuckSince`), gets one notification,
//! and plays the not-moving sound; all of it clears as soon as it shows output again.
//!
//! CDXC:Coordinators 2026-11-05 WHY:
//! Issue #427 (DaveW001): a thread's command hung while it wrote test files, its transcript got no new row for about 90 minutes, and its coordinator heard nothing, because reports only arrive when a turn ends; `coordinator status` said "working" with no time to judge by.
//! The transcript alone cannot tell a hang from a long build: Claude Code writes no row while a command runs, and its screen shows only "(8s · 9 lines)" under the call (seen live 2026-11-05). So a change on the screen counts as output too, fingerprinted without what ticks on its own (spinner glyphs, the turn timer, token counts, percentages, costs, clocks); a line count, a test's dots or a new compile line still change it. A screen that changes only through noise this filter does not know reads as busy, so that session goes unreported, as before.
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User, reviewing the first version (one report at 15 minutes): 8 minutes without output; at 8 minutes Ghostex nudges the thread itself ("Still working? Say what you're waiting on, or carry on.") and tells the coordinator in the same report, which does the healing (Ghostex never interrupts or reloads); the coordinator hears again every 8 minutes until the thread moves; a thread still not moving 8 minutes after the nudge gets the red dot on its sidebar and board rows, a notification and a sound of its own; and "put red dot on threads that aren't moving in general", so every other agent session gets the dot, notification and sound at 8 minutes, with no nudge or report since nobody heals it. The minutes stay a constant: no setting covers similar supervisor timings.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::records::SessionKey;
use crate::session_chat::{SessionChatBlock, SessionChatMessage, SessionChatRole};
use crate::session_chat_tail::SessionChatTailPage;

/// One round of quiet: a thread is nudged and reported after one, reported again after each
/// further one, and stuck after two; any other session is stuck after one.
pub const QUIET_ROUND_MS: i64 = 8 * 60_000;
/// Rounds of quiet before a coordinator's thread counts as stuck (8 minutes after its nudge).
pub const THREAD_STUCK_ROUNDS: u32 = 2;
/// Rounds of quiet before any other agent session counts as stuck.
pub const SESSION_STUCK_ROUNDS: u32 = 1;
/// One round of quiet as it runs: `QUIET_ROUND_MS`, shortened only on a debug test server.
pub fn quiet_round_ms() -> i64 {
    test_scaled(QUIET_ROUND_MS)
}

/// A timing divided by `GXSERVER_TEST_STALL_SCALE` (say 8 for one-minute rounds) in a debug build,
/// so `cargo xtask test-server` can show a whole stall in minutes. Release builds ignore it, and the
/// user's own gxserver never sees it.
pub fn test_scaled(ms: i64) -> i64 {
    if !cfg!(debug_assertions) {
        return ms;
    }
    static SCALE: OnceLock<i64> = OnceLock::new();
    let scale = *SCALE.get_or_init(|| {
        std::env::var("GXSERVER_TEST_STALL_SCALE")
            .ok()
            .and_then(|value| value.trim().parse::<i64>().ok())
            .filter(|scale| *scale > 1)
            .unwrap_or(1)
    });
    (ms / scale).max(1_000)
}

/// Whole minutes in a timing, as reports and rows say them (at least 1).
pub fn minutes_of(ms: i64) -> i64 {
    (ms / 60_000).max(1)
}

/// The message Ghostex sends a thread at its first round of quiet.
pub fn nudge_message() -> String {
    format!(
        "Ghostex: no new output from you for {} minutes. Still working? Say what you're waiting on, or carry on.",
        minutes_of(quiet_round_ms())
    )
}
/// How often one session's transcript (and, once it is quiet, its screen) is looked at.
pub(super) const STALL_CHECK_EVERY_MS: i64 = 30_000;
/// The screen is only fingerprinted once the session has been quiet this long: one that writes
/// rows needs no capture.
pub(super) const SCREEN_SAMPLE_AFTER_MS: i64 = 60_000;
/// After a nudge, what Ghostex's own message does (typed into the input box, queued on screen, a
/// row in an agent that records a message it takes mid-turn) is not the thread moving.
pub(super) const NUDGE_GRACE_MS: i64 = 90_000;
/// Rows read to learn the newest row's time.
const QUIET_TAIL_ROWS: usize = 4;
/// Rows read back to find the last row worth naming in the report.
const LAST_ROW_TAIL_ROWS: usize = 30;
const LAST_ROW_MAX_CHARS: usize = 240;

/// One working session's output clock, kept while it works.
pub struct StallWatch {
    /// The agent run it watches: its conversation and transcript, and its last launch (launch,
    /// resume, wake or reload). A different one starts the clock again (`agent_run_of`).
    run: Option<String>,
    /// When this working stint started being watched: nothing before it is known.
    watched_since: i64,
    path: Option<PathBuf>,
    /// The newest transcript write seen (file time or newest row, whichever is later).
    transcript_seen: Option<i64>,
    screen: Option<u64>,
    /// The last output counted: a transcript write or a screen change outside a nudge's grace.
    last_output_at: Option<i64>,
    checked_at: Option<i64>,
    /// The quiet stretch last reported, by when it began, and how many rounds of it were.
    reported: Option<(i64, u32)>,
    nudged_at: Option<i64>,
}

/// A report a watch says is due: the quiet stretch's start, which round of it this is (1 at the
/// first 8 minutes), and the transcript write it was judged against.
#[derive(Clone, Copy, Debug)]
pub struct StallDue {
    pub quiet_since: i64,
    pub round: u32,
    pub transcript_seen: Option<i64>,
}

impl StallWatch {
    pub fn new(now: i64) -> Self {
        Self {
            run: None,
            watched_since: now,
            path: None,
            transcript_seen: None,
            screen: None,
            last_output_at: None,
            checked_at: None,
            reported: None,
            nudged_at: None,
        }
    }

    /// BLOCKING (a file stat, a short transcript tail read and, once quiet, one screen capture),
    /// at most every `STALL_CHECK_EVERY_MS`. Returns the report due now, if any.
    pub fn observe(&mut self, session: &Value, now: i64) -> Option<StallDue> {
        // CDXC:Coordinators 2026-11-06 WHY: four threads the coordinator reloaded at 06:02 and that
        // were visibly working again were reported at 06:09 as "no new output for 8 minutes (quiet
        // since 02:01:04Z)". The watch outlived the reload: it kept the transcript file it had
        // resolved before (a reload that resumes on another account writes another file), so it
        // read a file that no longer moved, and its quiet clock still started before the reload.
        // A reload, restart or wake, or a new conversation or transcript, is a new agent run: the
        // watch starts over and resolves the transcript again.
        let run = agent_run_of(session);
        if self.run.as_ref().is_some_and(|known| *known != run) {
            *self = Self::new(now);
        }
        self.run = Some(run);
        if self
            .checked_at
            .is_none_or(|checked| now - checked >= test_scaled(STALL_CHECK_EVERY_MS))
        {
            self.checked_at = Some(now);
            self.look(session, now);
        }
        let quiet_since = self.quiet_since();
        let round = self.round(now);
        let reported = match self.reported {
            Some((since, rounds)) if since == quiet_since => rounds,
            _ => 0,
        };
        (round > reported).then_some(StallDue {
            quiet_since,
            round,
            transcript_seen: self.transcript_seen,
        })
    }

    fn look(&mut self, session: &Value, now: i64) {
        let in_grace = self
            .nudged_at
            .is_some_and(|nudged| now - nudged < test_scaled(NUDGE_GRACE_MS));
        let (transcript_at, own_row_at) = transcript_activity(session, &mut self.path);
        if transcript_at > self.transcript_seen {
            self.transcript_seen = transcript_at;
            // The screen is compared afresh once the rows stop.
            self.screen = None;
            if !in_grace {
                self.count_output(transcript_at);
            } else if own_row_at.is_some_and(|at| self.nudged_at.is_some_and(|nudged| at > nudged))
            {
                // Its own answer to the nudge is output; the nudge's row is not.
                self.count_output(own_row_at);
            }
        }
        if now - self.quiet_since() < test_scaled(SCREEN_SAMPLE_AFTER_MS) {
            return;
        }
        let Some(fingerprint) = screen_fingerprint_of(session) else {
            return;
        };
        if !in_grace && self.screen.is_some_and(|previous| previous != fingerprint) {
            self.count_output(Some(now));
        }
        self.screen = Some(fingerprint);
    }

    fn count_output(&mut self, at: Option<i64>) {
        if at > self.last_output_at {
            self.last_output_at = at;
            // A nudge belongs to the quiet stretch it was sent in.
            self.nudged_at = None;
        }
    }

    /// Whole rounds of quiet so far.
    fn round(&self, now: i64) -> u32 {
        ((now - self.quiet_since()).max(0) / quiet_round_ms()) as u32
    }

    /// Since when the session has shown no output, as far as this watch knows.
    pub fn quiet_since(&self) -> i64 {
        self.watched_since.max(self.last_output_at.unwrap_or(0))
    }

    pub fn mark_reported(&mut self, quiet_since: i64, round: u32) {
        self.reported = Some((quiet_since, round));
    }

    pub fn note_nudged(&mut self, now: i64) {
        self.nudged_at = Some(now);
    }

    /// What the panel, the board, `coordinator status` and the sidebar show; `stuck_rounds` is
    /// `THREAD_STUCK_ROUNDS` or `SESSION_STUCK_ROUNDS`.
    pub fn view(&self, now: i64, stuck_rounds: u32) -> SessionQuiet {
        let round = self.round(now);
        SessionQuiet {
            last_output_at: self.last_output_at,
            quiet_since: self.quiet_since(),
            stalled: round >= 1,
            stuck: round >= stuck_rounds,
        }
    }
}

/// A working session's output clock as every surface reads it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SessionQuiet {
    pub last_output_at: Option<i64>,
    pub quiet_since: i64,
    /// Quiet for at least one round.
    pub stalled: bool,
    /// Quiet long enough for the red dot, the notification and the sound.
    pub stuck: bool,
}

/// Who keeps a session's watch: the coordinator supervisor for its threads, the sidebar-wide
/// watch for every other agent session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum QuietOwner {
    Threads,
    Sessions,
    /// Sessions whose terminal ended under their agent and that Ghostex could not restart
    /// (session_self_heal.rs): stuck from the moment it gave up.
    Heal,
}

type QuietMaps = HashMap<QuietOwner, HashMap<SessionKey, SessionQuiet>>;

fn quiet_maps() -> &'static Mutex<QuietMaps> {
    static QUIET: OnceLock<Mutex<QuietMaps>> = OnceLock::new();
    QUIET.get_or_init(|| Mutex::new(HashMap::new()))
}

/// What changed in the stuck set when an owner published: the sessions that became stuck, and
/// those that stopped being stuck (moved, finished, or left the watch).
#[derive(Default)]
pub struct StuckChanges {
    pub became_stuck: Vec<SessionKey>,
    pub cleared: Vec<SessionKey>,
}

/// Replaces an owner's output clocks with its latest reading.
pub fn replace_session_quiet(
    owner: QuietOwner,
    quiet: HashMap<SessionKey, SessionQuiet>,
) -> StuckChanges {
    let Ok(mut maps) = quiet_maps().lock() else {
        return StuckChanges::default();
    };
    let stuck_keys = |map: &HashMap<SessionKey, SessionQuiet>| {
        map.iter()
            .filter(|(_, quiet)| quiet.stuck)
            .map(|(key, _)| key.clone())
            .collect::<HashSet<_>>()
    };
    let before = maps.get(&owner).map(stuck_keys).unwrap_or_default();
    let after = stuck_keys(&quiet);
    maps.insert(owner, quiet);
    StuckChanges {
        became_stuck: after.difference(&before).cloned().collect(),
        cleared: before.difference(&after).cloned().collect(),
    }
}

/// The latest output clock of a working session, whoever keeps it.
pub fn session_quiet(project_id: &str, session_id: &str) -> Option<SessionQuiet> {
    let key = (project_id.to_string(), session_id.to_string());
    let maps = quiet_maps().lock().ok()?;
    [QuietOwner::Heal, QuietOwner::Threads, QuietOwner::Sessions]
        .iter()
        .find_map(|owner| maps.get(owner)?.get(&key).copied())
}

/// True while the coordinator supervisor keeps this session's watch (a working thread), so the
/// sidebar-wide watch leaves it alone.
pub fn watched_as_thread(project_id: &str, session_id: &str) -> bool {
    let key = (project_id.to_string(), session_id.to_string());
    quiet_maps()
        .lock()
        .ok()
        .and_then(|maps| {
            maps.get(&QuietOwner::Threads)
                .map(|map| map.contains_key(&key))
        })
        .unwrap_or(false)
}

/// Since when a stuck session has shown no output; `None` while it is not stuck.
pub fn session_stuck_since(project_id: &str, session_id: &str) -> Option<i64> {
    session_quiet(project_id, session_id)
        .filter(|quiet| quiet.stuck)
        .map(|quiet| quiet.quiet_since)
}

/// Whole minutes of quiet, as reports and rows say them.
pub fn quiet_minutes(quiet_since: i64, now: i64) -> i64 {
    ((now - quiet_since).max(0)) / 60_000
}

/// The words a panel or board row shows for a quiet thread.
pub fn quiet_detail(quiet: &SessionQuiet, now: i64) -> String {
    let rounds = (now - quiet.quiet_since).max(0) / quiet_round_ms();
    let minutes = minutes_of(rounds.max(1) * quiet_round_ms());
    if quiet.stuck {
        format!("Not moving: no new output for over {minutes} minutes")
    } else {
        format!("No new output for over {minutes} minutes")
    }
}

pub fn iso_from_ms(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// What names one agent run of a session: its conversation id and transcript path, and its last
/// launch, resume, wake or reload (`suppressedUntil`, which each of them stamps, see
/// server/src/session_status/model.rs).
fn agent_run_of(session: &Value) -> String {
    let runtime = |pointer: &str| {
        session
            .pointer(&format!("/runtimeSettings/{pointer}"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    format!(
        "{}|{}|{}",
        runtime("agentSessionId"),
        runtime("agentSessionPath"),
        runtime("agentActivity/suppressedUntil")
    )
}

/// When the session's agent was last launched, resumed, woken or reloaded (ms).
fn agent_launched_at(session: &Value) -> Option<i64> {
    let until = session
        .pointer("/runtimeSettings/agentActivity/suppressedUntil")
        .and_then(Value::as_str)?;
    let until = chrono::DateTime::parse_from_rfc3339(until).ok()?;
    Some(until.timestamp_millis() - crate::session_status::INITIAL_ACTIVITY_SUPPRESSION_MS)
}

/// BLOCKING. The newest transcript write: the file's modification time or its newest row's time,
/// whichever is later (a derived transcript such as OpenCode's is refreshed by the tail read).
fn transcript_activity_at(session: &Value, path: &mut Option<PathBuf>) -> Option<i64> {
    transcript_activity(session, path).0
}

/// BLOCKING. `transcript_activity_at`, and the newest row the agent wrote itself (not Ghostex's
/// nudge).
fn transcript_activity(session: &Value, path: &mut Option<PathBuf>) -> (Option<i64>, Option<i64>) {
    let Some(agent) = super::delivery::resolve_thread_transcript(session, path) else {
        return (None, None);
    };
    let Some(path) = path.as_ref() else {
        return (None, None);
    };
    let messages = match crate::session_chat_tail::read_session_chat_tail_page(
        agent,
        path,
        QUIET_TAIL_ROWS,
        None,
    ) {
        Ok(SessionChatTailPage::Page { messages, .. }) => messages,
        _ => Vec::new(),
    };
    let newest_row = messages
        .iter()
        .filter_map(|message| message.timestamp)
        .max();
    let own_row = messages
        .iter()
        .filter(|message| !is_ghostex_nudge(message))
        .filter_map(|message| message.timestamp)
        .max();
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64);
    (modified.max(newest_row), own_row)
}

/// BLOCKING (one screenful from the terminal daemon). `None` when the screen cannot be read.
fn screen_fingerprint_of(session: &Value) -> Option<u64> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session).ok()?;
    let capture = crate::zmx::read_zmx_session_grid_capture(&zmx_name).ok()?;
    (!capture.truncated).then(|| screen_fingerprint(&capture.text))
}

/// A screen's text without what changes on its own while nothing happens.
pub(super) fn screen_fingerprint(text: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for line in text.lines() {
        let words = line
            .split_whitespace()
            .map(|word| {
                word.chars()
                    .filter(|ch| !is_spinner_glyph(*ch))
                    .collect::<String>()
            })
            .filter(|word| !word.is_empty() && !is_ticking_number(word))
            .collect::<Vec<_>>();
        if !words.is_empty() {
            words.join(" ").hash(&mut hasher);
        }
    }
    hasher.finish()
}

/// Spinner frames and the blinking marks agents draw beside a running step.
fn is_spinner_glyph(ch: char) -> bool {
    matches!(
        ch,
        '·' | '✢'
            | '✳'
            | '✶'
            | '✻'
            | '✽'
            | '*'
            | '⏺'
            | '•'
            | '◦'
            | '●'
            | '○'
            | '◐'
            | '◓'
            | '◑'
            | '◒'
            | '◴'
            | '◵'
            | '◶'
            | '◷'
            | '|'
            | '/'
            | '\\'
            | '-'
    ) || ('\u{2800}'..='\u{28ff}').contains(&ch)
}

/// A number that ticks by itself on a waiting screen: a duration ("3m", "39s", "1m05s"), a token
/// count ("13.0k"), a percentage, a cost or a clock. A bare count ("9" lines, a test's "41/300")
/// is progress and is kept.
fn is_ticking_number(word: &str) -> bool {
    let word =
        word.trim_matches(|ch: char| matches!(ch, '(' | ')' | '[' | ']' | ',' | ';' | '↓' | '↑'));
    if let Some(amount) = word.strip_prefix('$') {
        return amount.starts_with(|ch: char| ch.is_ascii_digit());
    }
    if !word.starts_with(|ch: char| ch.is_ascii_digit()) {
        return false;
    }
    let split = word
        .find(|ch: char| !(ch.is_ascii_digit() || ch == '.' || ch == ':'))
        .unwrap_or(word.len());
    let (number, suffix) = word.split_at(split);
    if number.contains(':') {
        return suffix.is_empty();
    }
    match suffix {
        "" => number.contains('.'),
        "k" | "K" | "M" | "%" => true,
        _ => is_duration(word),
    }
}

/// "39s", "3m", "1h2m", "120ms", "5mins": numbers each followed by a time unit.
fn is_duration(word: &str) -> bool {
    let mut rest = word;
    while !rest.is_empty() {
        let number = rest
            .find(|ch: char| !(ch.is_ascii_digit() || ch == '.'))
            .unwrap_or(rest.len());
        if number == 0 {
            return false;
        }
        rest = &rest[number..];
        let unit = rest
            .find(|ch: char| !ch.is_ascii_alphabetic())
            .unwrap_or(rest.len());
        if !matches!(
            &rest[..unit],
            "ms" | "s" | "m" | "h" | "d" | "sec" | "secs" | "min" | "mins"
        ) {
            return false;
        }
        rest = &rest[unit..];
    }
    true
}

/// BLOCKING (a transcript tail read). Read again right before a stall report goes out, from the
/// session as it is now: `None` when the report no longer holds and is dropped (the transcript
/// moved since the report was worded, it has a row newer than the quiet time the report names,
/// or the agent was launched, resumed or reloaded since then), else the last row worth naming,
/// when there is one. Ghostex's own nudge is no sign of life.
pub fn stall_last_row(
    session: &Value,
    transcript_seen: Option<i64>,
    quiet_since: i64,
) -> Option<Option<String>> {
    if agent_launched_at(session).is_some_and(|launched| launched > quiet_since) {
        return None;
    }
    let mut path = None;
    if transcript_activity_at(session, &mut path) > transcript_seen {
        return None;
    }
    let agent = super::delivery::resolve_thread_transcript(session, &mut path);
    let messages = agent
        .zip(path.as_ref())
        .and_then(|(agent, path)| {
            crate::session_chat_tail::read_session_chat_tail_page(
                agent,
                path,
                LAST_ROW_TAIL_ROWS,
                None,
            )
            .ok()
        })
        .and_then(|page| match page {
            SessionChatTailPage::Page { messages, .. } => Some(messages),
            SessionChatTailPage::NotFound => None,
        })
        .unwrap_or_default();
    let moved = messages
        .iter()
        .filter(|message| !is_ghostex_nudge(message))
        .filter_map(|message| message.timestamp)
        .any(|at| at > quiet_since);
    if moved {
        return None;
    }
    Some(messages.iter().rev().find_map(describe_row))
}

/// A message Ghostex typed into the thread itself (a stall or idle nudge).
pub(super) fn is_ghostex_nudge(message: &SessionChatMessage) -> bool {
    message.role == SessionChatRole::User
        && message.blocks.iter().any(|block| {
            matches!(block, SessionChatBlock::Text { text } if text.trim_start().starts_with("Ghostex:"))
        })
}

/// One line naming a transcript row: the step it ran, what it said, or what a tool returned.
pub(super) fn describe_row(message: &SessionChatMessage) -> Option<String> {
    // A message to it (the brief, a follow-up, Ghostex's nudge) says nothing about where it is.
    if message.role == SessionChatRole::User {
        return None;
    }
    let line = message.blocks.iter().rev().find_map(|block| match block {
        SessionChatBlock::ToolCall { name, input, .. } => Some(match tool_call_subject(input) {
            Some(subject) => format!("It started {name}: {subject}"),
            None => format!("It started {name}"),
        }),
        SessionChatBlock::ToolResult {
            output, is_error, ..
        } => first_line(output).map(|line| {
            if *is_error == Some(true) {
                format!("A tool failed: {line}")
            } else {
                format!("A tool returned: {line}")
            }
        }),
        SessionChatBlock::Text { text } => first_line(text).map(|line| format!("It wrote: {line}")),
        SessionChatBlock::ImageRef { .. } => None,
    })?;
    let mut line = line
        .chars()
        .take(LAST_ROW_MAX_CHARS + 1)
        .collect::<String>();
    if line.chars().count() > LAST_ROW_MAX_CHARS {
        line = line.chars().take(LAST_ROW_MAX_CHARS).collect::<String>() + "…";
    }
    Some(line)
}

/// What a tool call works on: its command, file, pattern or prompt.
fn tool_call_subject(input: &Value) -> Option<String> {
    [
        "command",
        "cmd",
        "file_path",
        "path",
        "pattern",
        "url",
        "query",
        "description",
        "prompt",
    ]
    .iter()
    .find_map(|key| match input.get(*key)? {
        Value::String(text) => first_line(text),
        Value::Array(parts) => first_line(
            &parts
                .iter()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(" "),
        ),
        _ => None,
    })
}

fn first_line(text: &str) -> Option<String> {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
}
