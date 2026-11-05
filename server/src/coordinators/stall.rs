//! A working thread that has gone quiet: no new transcript row and no change on its screen for a
//! long stretch. The supervisor (server/coordinator_runtime.rs) keeps one `StallWatch` per working
//! thread and sends its coordinator one "no new output" report per stall; the Threads panel, the
//! board and `ghostex coordinator status` read the same watch through `thread_quiet`.
//!
//! CDXC:Coordinators 2026-11-05 WHY:
//! Issue #427 (DaveW001): a thread's command hung while it wrote test files, its transcript got no new row for about 90 minutes, and its coordinator heard nothing, because reports only arrive when a turn ends; `coordinator status` said "working" with no time to judge by. A thread that works this long without output now gets one report, and status and the board show since when it has been quiet. Recovery stays with the coordinator (the issue asks for no automatic reload).
//! The transcript alone cannot tell a hang from a long build: Claude Code writes no row while a command runs, and its screen shows only "(8s · 9 lines)" under the call (seen live 2026-11-05). So a change on the screen counts as output too, fingerprinted without what ticks on its own (spinner glyphs, the turn timer, token counts, percentages, costs, clocks); a line count, a test's dots or a new compile line still change it. A screen that changes only through noise this filter does not know reads as busy, so that thread goes unreported, as before.
//! DECISION: 15 minutes, a constant (no setting covers similar supervisor timings; they are all constants here). The issue asked for 10 or 15; 15 stays clear of a long reasoning step or a slow API retry, which also write nothing for minutes, while still catching a hang long before the 90 minutes reported.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use serde_json::Value;

use super::records::SessionKey;
use crate::session_chat::{SessionChatBlock, SessionChatMessage};
use crate::session_chat_tail::SessionChatTailPage;

/// How long a working thread must be quiet before its coordinator is told.
pub const STALL_AFTER_MS: i64 = 15 * 60_000;
/// How often one thread's transcript (and, once it is quiet, its screen) is looked at.
const STALL_CHECK_EVERY_MS: i64 = 30_000;
/// The screen is only fingerprinted once the transcript has been quiet this long: a thread that
/// writes rows needs no capture.
const SCREEN_SAMPLE_AFTER_MS: i64 = 60_000;
/// Rows read to learn the newest row's time.
const QUIET_TAIL_ROWS: usize = 4;
/// Rows read back to find the last row worth naming in the report.
const LAST_ROW_TAIL_ROWS: usize = 30;
const LAST_ROW_MAX_CHARS: usize = 240;

/// One working thread's output clock, kept by the supervisor while the thread works.
pub struct StallWatch {
    /// When the supervisor started watching this working stint: nothing before it is known.
    watched_since: i64,
    path: Option<PathBuf>,
    /// The newest transcript write seen (file time or newest row, whichever is later).
    transcript_at: Option<i64>,
    screen: Option<u64>,
    /// When the screen fingerprint was last seen to change.
    screen_changed_at: Option<i64>,
    checked_at: Option<i64>,
    /// The quiet stretch already reported, by when it began.
    reported_quiet_since: Option<i64>,
}

impl StallWatch {
    pub fn new(now: i64) -> Self {
        Self {
            watched_since: now,
            path: None,
            transcript_at: None,
            screen: None,
            screen_changed_at: None,
            checked_at: None,
            reported_quiet_since: None,
        }
    }

    /// BLOCKING (a file stat, a short transcript tail read and, once quiet, one screen capture),
    /// at most every `STALL_CHECK_EVERY_MS`. Returns when the current quiet stretch began if it is a
    /// stall not yet reported.
    pub fn observe(&mut self, session: &Value, now: i64) -> Option<i64> {
        if self
            .checked_at
            .is_none_or(|checked| now - checked >= STALL_CHECK_EVERY_MS)
        {
            self.checked_at = Some(now);
            self.look(session, now);
        }
        let quiet_since = self.quiet_since();
        (now - quiet_since >= STALL_AFTER_MS && self.reported_quiet_since != Some(quiet_since))
            .then_some(quiet_since)
    }

    fn look(&mut self, session: &Value, now: i64) {
        let transcript_at = transcript_activity_at(session, &mut self.path);
        if transcript_at > self.transcript_at {
            self.transcript_at = transcript_at;
            // New rows are output enough; the screen is compared afresh once they stop.
            self.screen = None;
        }
        let transcript_quiet_since = self.watched_since.max(self.transcript_at.unwrap_or(0));
        if now - transcript_quiet_since < SCREEN_SAMPLE_AFTER_MS {
            return;
        }
        let Some(fingerprint) = screen_fingerprint_of(session) else {
            return;
        };
        if self.screen.is_some_and(|previous| previous != fingerprint) {
            self.screen_changed_at = Some(now);
        }
        self.screen = Some(fingerprint);
    }

    /// The last output seen: a transcript write or a screen change.
    pub fn last_output_at(&self) -> Option<i64> {
        self.transcript_at.max(self.screen_changed_at)
    }

    /// Since when the thread has shown no output, as far as this watch knows.
    pub fn quiet_since(&self) -> i64 {
        self.watched_since.max(self.last_output_at().unwrap_or(0))
    }

    pub fn mark_reported(&mut self, quiet_since: i64) {
        self.reported_quiet_since = Some(quiet_since);
    }

    pub fn view(&self, now: i64) -> ThreadQuiet {
        let quiet_since = self.quiet_since();
        ThreadQuiet {
            last_output_at: self.last_output_at(),
            quiet_since,
            stalled: now - quiet_since >= STALL_AFTER_MS,
        }
    }
}

/// What the panel, the board and `coordinator status` show about a working thread's output.
#[derive(Clone, Copy, Debug)]
pub struct ThreadQuiet {
    pub last_output_at: Option<i64>,
    pub quiet_since: i64,
    pub stalled: bool,
}

fn thread_quiet_map() -> &'static Mutex<HashMap<SessionKey, ThreadQuiet>> {
    static QUIET: OnceLock<Mutex<HashMap<SessionKey, ThreadQuiet>>> = OnceLock::new();
    QUIET.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Replaces every working thread's output clock with the supervisor's latest reading.
pub fn replace_thread_quiet(quiet: HashMap<SessionKey, ThreadQuiet>) {
    if let Ok(mut current) = thread_quiet_map().lock() {
        *current = quiet;
    }
}

/// The supervisor's latest output clock for a working thread.
pub fn thread_quiet(project_id: &str, session_id: &str) -> Option<ThreadQuiet> {
    thread_quiet_map()
        .lock()
        .ok()?
        .get(&(project_id.to_string(), session_id.to_string()))
        .copied()
}

/// The words a panel or board row shows for a stalled thread.
pub fn stalled_detail() -> String {
    format!("No new output for over {} minutes", STALL_AFTER_MS / 60_000)
}

pub fn iso_from_ms(ms: i64) -> Option<String> {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|at| at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}

/// BLOCKING. The newest transcript write: the file's modification time or its newest row's time,
/// whichever is later (a derived transcript such as OpenCode's is refreshed by the tail read).
fn transcript_activity_at(session: &Value, path: &mut Option<PathBuf>) -> Option<i64> {
    let agent = super::delivery::resolve_thread_transcript(session, path)?;
    let path = path.as_ref()?;
    let newest_row = match crate::session_chat_tail::read_session_chat_tail_page(
        agent,
        path,
        QUIET_TAIL_ROWS,
        None,
    ) {
        Ok(SessionChatTailPage::Page { messages, .. }) => messages
            .iter()
            .filter_map(|message| message.timestamp)
            .max(),
        _ => None,
    };
    let modified = std::fs::metadata(path)
        .and_then(|metadata| metadata.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|elapsed| elapsed.as_millis() as i64);
    modified.max(newest_row)
}

/// BLOCKING (one screenful from the terminal daemon). `None` when the screen cannot be read.
fn screen_fingerprint_of(session: &Value) -> Option<u64> {
    let zmx_name = crate::zmx::provider_zmx_session_name(session).ok()?;
    let capture = crate::zmx::read_zmx_session_grid_capture(&zmx_name).ok()?;
    (!capture.truncated).then(|| screen_fingerprint(&capture.text))
}

/// A screen's text without what changes on its own while nothing happens.
fn screen_fingerprint(text: &str) -> u64 {
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

/// BLOCKING (a transcript tail read). Read again right before a stall report goes out: `None` when
/// the transcript moved since the stall began (the report is dropped), else the last row worth
/// naming, when there is one.
pub fn stall_last_row(session: &Value, quiet_since: i64) -> Option<Option<String>> {
    let mut path = None;
    if transcript_activity_at(session, &mut path).is_some_and(|at| at > quiet_since) {
        return None;
    }
    let row = (|| {
        let agent = super::delivery::resolve_thread_transcript(session, &mut path)?;
        let page = crate::session_chat_tail::read_session_chat_tail_page(
            agent,
            path.as_ref()?,
            LAST_ROW_TAIL_ROWS,
            None,
        )
        .ok()?;
        let SessionChatTailPage::Page { messages, .. } = page else {
            return None;
        };
        messages.iter().rev().find_map(describe_row)
    })();
    Some(row)
}

/// One line naming a transcript row: the step it ran, what it said, or what a tool returned.
fn describe_row(message: &SessionChatMessage) -> Option<String> {
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
