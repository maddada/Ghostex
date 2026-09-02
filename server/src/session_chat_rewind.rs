/*
CDXC:SessionChatRewind 2026-09-02:
`/api/rewindSessionChat` drives Claude Code's OWN rewind flow, in the session's
terminal, exactly the way a human drives it: type `/rewind`, walk the prompt
list up to the chosen prompt, press Enter, pick the option whose label is
exactly `Restore conversation`, press Enter again. There is no file to edit and
no API to call: the rewind lives in the CLI's in-memory conversation, and the
transcript only records it later, when the next prompt is appended with the
rewound leaf as its `parentUuid`. So the terminal IS the interface.

Driving a TUI blind is how keystrokes end up in the wrong dialog, so nothing
here is written on faith. Every step is followed by a screen capture that must
prove the expected dialog is up and the expected row is highlighted before the
next byte goes out, and any capture that disagrees aborts the drive with Escape
instead of continuing. The prompt the caller named is matched against the text
the TUI itself is showing on the highlighted row, so a list that is one row off
(a transcript row Claude counts differently, a rewind somebody did by hand in
the terminal) is refused rather than silently rewinding to the wrong turn.

The whole sequence runs as ONE job on the per-session send worker
(`SessionChatSendStep::DriveClaudeRewind`), for the reason
CDXC:SessionChatSerializedWriters states: a queued prompt landing between the
Up presses would type into the rewind dialog. It also means an interrupt that
bumps the session's send generation aborts the drive at its next step.

On success the target's `parentUuid` is recorded as the session's pending
rewind (session_chat_rewind_state.rs) so the chat readers hide the rewound rows
immediately, before the transcript can confirm anything.
*/

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::protocol::rpc_success;
use crate::server::{
    domain_error_response, read_runtime_text, routed_json, AppState, RoutedResponse,
};
use crate::session_chat::{SessionChatRole, SessionChatTranscriptAgent, TranscriptLineage};
use crate::session_chat_decode_claude::{
    claude_transcript_lineage, decode_claude_transcript_line, is_noise_message, message_text,
};
use crate::session_chat_rewind_state::{
    session_chat_pending_rewind, set_session_chat_pending_rewind, SessionChatPendingRewind,
};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, resolve_session_chat_send_target,
    write_session_chat_payload, SessionChatSendStep, SESSION_CHAT_INTERRUPT,
};

/// What the driver writes to open the flow. Typed as literal keystrokes, not a
/// bracketed paste: this is a slash command the composer must autocomplete and
/// run, which is exactly what a paste frame would suppress.
const CLAUDE_REWIND_COMMAND: &str = "/rewind";

/// Up arrow, the only key that moves the prompt list. The rewind list is walked
/// in ONE direction on purpose: an overshoot cannot be corrected without
/// re-reading the whole list, so it is a refusal instead.
const CLAUDE_REWIND_LIST_UP: &str = "\u{1b}[A";

/// Down arrow, used only inside the confirmation menu, where the option this
/// driver wants is always at or below the initially highlighted row.
const CLAUDE_REWIND_MENU_DOWN: &str = "\u{1b}[B";

const CLAUDE_REWIND_SUBMIT: &str = "\r";

/// The dialog's own title line, and the two subtitles that tell its two stages
/// apart. Matched on the space-collapsed line so a repaint that pads
/// differently still reads the same.
const CLAUDE_REWIND_TITLE: &str = "Rewind";
const CLAUDE_REWIND_LIST_SUBTITLE: &str = "Restore the code and/or conversation";
const CLAUDE_REWIND_CONFIRM_SUBTITLE: &str =
    "Confirm you want to restore to the point before you sent this message";

/// The option this driver is allowed to choose, matched EXACTLY. Claude offers
/// `Restore code and conversation` right above it whenever the turn touched
/// files, and that option reverts the user's working tree. Nothing here may
/// ever accept a fuzzy match against this label.
const CLAUDE_REWIND_RESTORE_CONVERSATION: &str = "Restore conversation";

/// Highlight marker for both the prompt list and the confirmation menu.
const CLAUDE_REWIND_CURSOR: char = '\u{276f}';
/// Quote marker on the confirmation dialog's echo of the target prompt.
const CLAUDE_REWIND_QUOTE: char = '\u{2502}';
/// Claude's truncation ellipsis on a long prompt row.
const CLAUDE_REWIND_ELLIPSIS: char = '\u{2026}';

/// Poll cadence for every screen wait, matching the composer and paste waits.
const REWIND_POLL_MS: u64 = 150;
/// Ceiling on ONE step: opening the dialog, one highlight move, one menu move,
/// or the teardown after the final Enter.
const REWIND_STEP_TIMEOUT_MS: u64 = 6_000;
/// Settle between typing the command and submitting it, so the two writes reach
/// the TUI in separate stdin chunks (the same chunk-separation rule the clear
/// burst follows in session_chat_send.rs).
const REWIND_COMMAND_SETTLE_MS: u64 = 300;
/// Extra Up presses allowed past the counted position before the drive gives
/// up. The count comes from the transcript and the highlight comes from the
/// TUI; a small disagreement is walked off, a large one is a refusal.
const REWIND_EXTRA_PRESSES: usize = 3;
/// Ceiling on Down presses inside the confirmation menu. The longest measured
/// menu has five options and starts on the first.
const REWIND_MENU_MAX_PRESSES: usize = 8;
/*
How long a move key is given to repaint the highlight before the drive treats
the highlight as having STAYED PUT. This is not a timeout: a list already
sitting on its oldest prompt, and a menu already on its last option, answer a
move key by doing nothing at all, and there is no marker on screen that says so
in every case (a conversation short enough to fit the window never paints
`↑ N more above`). So a move that does not land inside this window is reported
as "did not move", which the callers turn into a match check and then a refusal,
instead of into a six-second stall per press.
*/
const REWIND_MOVE_SETTLE_MS: u64 = 1_200;
/// Escapes written to close whatever is on screen after a failed drive. Two,
/// because the confirmation stage backs out to the prompt list first.
const REWIND_CANCEL_ESCAPES: usize = 2;
/// Full-width rule characters that frame Claude's composer.
const COMPOSER_RULE: char = '\u{2500}';
/// Shortest run of rule characters that can be composer chrome rather than an
/// inline separator inside prose.
const COMPOSER_MIN_RULE_CHARS: usize = 20;
/// Tallest input row this reads as a composer. Claude grows the box with the
/// draft, and past this height a pair of rules is far more likely to be two
/// unrelated separators with conversation between them.
const COMPOSER_MAX_DRAFT_LINES: usize = 20;

static SESSION_CHAT_REWIND_LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

fn log_session_chat_rewind(level: LogLevel, event: &str, details: Value, error: Option<String>) {
    let logger = SESSION_CHAT_REWIND_LOGGER
        .get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
    let _ = logger.log(GxserverLogInput {
        level,
        event: event.to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error,
        details: Some(details),
    });
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

fn unsupported_agent() -> DomainStateError {
    DomainStateError {
        code: "unsupportedAgent",
        message: "Rewind is only available for Claude Code sessions.".to_string(),
    }
}

fn session_not_running(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "sessionNotRunning",
        message: message.into(),
    }
}

fn agent_busy(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "agentBusy",
        message: message.into(),
    }
}

fn message_not_found(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "messageNotFound",
        message: message.into(),
    }
}

fn dialog_mismatch(step: &str, detail: &str) -> DomainStateError {
    DomainStateError {
        code: "dialogMismatch",
        message: format!("The rewind dialog did not show what was expected at the {step} step: {detail} Nothing was rewound."),
    }
}

fn rewind_timeout(step: &str) -> DomainStateError {
    DomainStateError {
        code: "timeout",
        message: format!(
            "Claude Code did not answer the rewind {step} step in time. Nothing was rewound."
        ),
    }
}

// ---------------------------------------------------------------------------
// Screen text helpers
// ---------------------------------------------------------------------------

/// One screen line with its runs of whitespace (including the non-breaking
/// spaces zmx captures inside statuslines) collapsed to single spaces, trimmed.
fn collapse_spaces(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn screen_lines(screen: &str) -> Vec<String> {
    screen.split('\n').map(collapse_spaces).collect()
}

/// The dialog's own region of the screen: everything from the LAST line that is
/// just the `Rewind` title. Taking the last one is what keeps the conversation
/// scrollback out of the parsers, which matters because the scrollback carries
/// `❯ <prompt>` echoes that look exactly like a highlighted list row.
fn rewind_dialog_region(lines: &[String]) -> Option<&[String]> {
    let title = lines.iter().rposition(|line| line == CLAUDE_REWIND_TITLE)?;
    Some(&lines[title..])
}

/// The dialog region, but only when it is the stage `subtitle` names.
fn rewind_stage_region<'a>(lines: &'a [String], subtitle: &str) -> Option<&'a [String]> {
    let region = rewind_dialog_region(lines)?;
    region
        .iter()
        .any(|line| line.starts_with(subtitle))
        .then_some(region)
}

/// Text of the highlighted row, i.e. the one the `❯` marker owns.
fn highlighted_row(region: &[String]) -> Option<String> {
    region.iter().find_map(|line| {
        let rest = line.strip_prefix(CLAUDE_REWIND_CURSOR)?;
        Some(rest.trim().to_string())
    })
}

/// Whether `row`, as the TUI painted it, is showing `target_first_line`.
///
/// A row Claude had to shorten ends in `…`, and only such a row is matched as a
/// prefix; every other row must be the whole first line. Accepting a prefix
/// unconditionally would make `test 3` match the prompt `test 33` sitting one
/// row away from it, and the price of that mistake is rewinding away turns the
/// user meant to keep. Refusing an unmarked truncation is the safe direction:
/// the caller gets `dialogMismatch` and nothing is rewound.
fn row_matches_prompt(row: &str, target_first_line: &str) -> bool {
    match row.strip_suffix(CLAUDE_REWIND_ELLIPSIS) {
        Some(head) => {
            let head = head.trim_end();
            !head.is_empty() && target_first_line.starts_with(head)
        }
        None => !row.is_empty() && row == target_first_line,
    }
}

/// The first line of a prompt, space-collapsed. The list paints one line per
/// prompt, so this is the only part of a multi-line prompt a row can show.
fn prompt_first_line(text: &str) -> String {
    collapse_spaces(text.split(['\r', '\n']).next().unwrap_or_default())
}

/// The prompt the confirmation dialog is quoting back, as its `│` lines show
/// it. The trailing `│ (12m ago)` row is the relative time Claude appends, not
/// part of the prompt.
fn confirm_quoted_first_line(region: &[String]) -> Option<String> {
    let quoted: Vec<String> = region
        .iter()
        .filter_map(|line| {
            let rest = line.strip_prefix(CLAUDE_REWIND_QUOTE)?;
            Some(rest.trim().to_string())
        })
        .collect();
    let first = quoted.first()?;
    let is_only_a_timestamp = quoted.len() == 1 && first.starts_with('(') && first.ends_with(')');
    (!is_only_a_timestamp).then(|| first.clone())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RewindMenuOption {
    number: u32,
    label: String,
    selected: bool,
}

/// The numbered options of the confirmation menu, in screen order. A scrolled
/// menu prefixes its edge rows with `↑`/`↓`, so those markers are accepted in
/// the same position the `❯` cursor uses.
fn parse_menu_options(region: &[String]) -> Vec<RewindMenuOption> {
    let mut options = Vec::new();
    for line in region {
        let mut rest = line.as_str();
        let mut selected = false;
        if let Some(stripped) = rest.strip_prefix(CLAUDE_REWIND_CURSOR) {
            selected = true;
            rest = stripped.trim_start();
        } else if let Some(stripped) = rest
            .strip_prefix('\u{2193}')
            .or_else(|| rest.strip_prefix('\u{2191}'))
        {
            rest = stripped.trim_start();
        }
        let Some(dot) = rest.find('.') else {
            continue;
        };
        let Ok(number) = rest[..dot].parse::<u32>() else {
            continue;
        };
        let label = rest[dot + 1..].trim();
        if label.is_empty() {
            continue;
        }
        options.push(RewindMenuOption {
            number,
            label: label.to_string(),
            selected,
        });
    }
    options
}

/// The draft sitting in Claude's composer, or `None` when the composer frame is
/// not on screen at all: a dialog owns the screen, or the CLI is painting
/// something else. An empty composer answers `Some("")`.
///
/// The frame is the rule sandwich session_chat_composer.rs matches for Claude
/// (`─` rule, `❯` input row, `─` rule), read here rather than there because
/// this driver needs the CONTENT of the input row, not just its presence. Rows
/// between the marker row and the closing rule are the wrapped rest of a
/// multi-line draft and are folded into one line, so `❯ a` over `b` reads as
/// `a b`.
fn composer_draft(screen: &str) -> Option<String> {
    let lines: Vec<String> = screen
        .split('\n')
        .map(|line| line.trim_end().to_string())
        .filter(|line| !line.trim().is_empty())
        .collect();
    let rules: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| {
            line.chars().filter(|value| *value == COMPOSER_RULE).count() >= COMPOSER_MIN_RULE_CHARS
        })
        .map(|(index, _)| index)
        .collect();
    for pair in (1..rules.len()).rev() {
        let (open, close) = (rules[pair - 1], rules[pair]);
        let body = &lines[open + 1..close];
        if body.is_empty() || body.len() > COMPOSER_MAX_DRAFT_LINES {
            continue;
        }
        let Some(first) = body[0].trim_start().strip_prefix(CLAUDE_REWIND_CURSOR) else {
            continue;
        };
        let mut draft = collapse_spaces(first);
        for line in &body[1..] {
            let line = collapse_spaces(line);
            if line.is_empty() {
                continue;
            }
            if !draft.is_empty() {
                draft.push(' ');
            }
            draft.push_str(&line);
        }
        return Some(draft);
    }
    None
}

// ---------------------------------------------------------------------------
// Transcript: the active conversation and the target prompt
// ---------------------------------------------------------------------------

struct ClaudeTranscriptRow {
    lineage: TranscriptLineage,
    /// Real user prompt text, set only for rows the chat decoder publishes as
    /// user turns. Tool results, meta turns, and harness-injected envelopes are
    /// `None`, because Claude's rewind list does not offer them either.
    prompt_text: Option<String>,
}

struct ClaudeRewindTarget {
    /// `uuid` of the row the caller named.
    message_id: String,
    /// `parentUuid` of that row: the leaf the conversation sits on afterwards.
    leaf_id: Option<String>,
    /// First line of the prompt, space-collapsed, for screen verification.
    first_line: String,
    /// Real user prompts of the active conversation that come AFTER the target.
    /// The list starts on `(current)`, so reaching the target costs one more
    /// press than that.
    prompts_after: usize,
}

/// Every non-sidechain, non-queue row of a Claude transcript, in file order,
/// with the lineage and the decoded prompt text this driver needs.
///
/// This walks the file with the SAME two functions the chat tail reader
/// composes (`claude_transcript_lineage` for the tree, the Claude line decoder
/// plus the noise classifier for "is this a real user prompt"). It cannot use
/// the tail page itself, because a page carries decoded messages without their
/// parents, and the parent is precisely what a rewind is addressed by.
fn read_claude_transcript_rows(path: &Path) -> std::io::Result<Vec<ClaudeTranscriptRow>> {
    let contents = std::fs::read_to_string(path)?;
    let mut rows = Vec::new();
    let mut offset: u64 = 0;
    for line in contents.split_inclusive('\n') {
        let line_offset = offset;
        offset += line.len() as u64;
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.trim().is_empty() {
            continue;
        }
        let fallback_id = format!("{}:{line_offset:012}", path.to_string_lossy());
        let Some(lineage) = claude_transcript_lineage(trimmed, &fallback_id) else {
            continue;
        };
        if lineage.queue.is_some() {
            continue;
        }
        let prompt_text =
            decode_claude_transcript_line(trimmed, &fallback_id).and_then(|message| {
                let is_prompt = message.role == SessionChatRole::User
                    && !message.queued
                    && !is_noise_message(&message);
                is_prompt.then(|| message_text(&message))
            });
        rows.push(ClaudeTranscriptRow {
            lineage,
            prompt_text,
        });
    }
    Ok(rows)
}

/// `uuid` of the row the conversation is currently sitting on.
///
/// Normally that is simply the last row in the file. When THIS daemon drove a
/// rewind whose branch the agent has not written a prompt for yet, the last row
/// still belongs to the abandoned branch, so the recorded leaf is the truth
/// until something is appended past its cutoff.
fn active_leaf_id(path: &Path, rows: &[ClaudeTranscriptRow], file_len: u64) -> Option<String> {
    if let Some(pending) = session_chat_pending_rewind(path) {
        if file_len <= pending.cutoff_offset {
            return pending.leaf_id;
        }
    }
    rows.last().map(|row| row.lineage.id.clone())
}

/// The active conversation, oldest first: the chain of rows from the root down
/// to the active leaf. This is exactly the set Claude's rewind list is built
/// from, which is why rows on abandoned branches never enter the count.
fn active_conversation<'a>(
    rows: &'a [ClaudeTranscriptRow],
    leaf_id: Option<&str>,
) -> Vec<&'a ClaudeTranscriptRow> {
    let by_id: HashMap<&str, &ClaudeTranscriptRow> = rows
        .iter()
        .map(|row| (row.lineage.id.as_str(), row))
        .collect();
    let mut chain: Vec<&ClaudeTranscriptRow> = Vec::new();
    let mut cursor = leaf_id;
    while let Some(id) = cursor {
        let Some(row) = by_id.get(id) else {
            break;
        };
        chain.push(row);
        cursor = row.lineage.parent_id.as_deref();
        // A cycle cannot happen in a well-formed transcript, and a malformed
        // one must not spin the request forever.
        if chain.len() > rows.len() {
            break;
        }
    }
    chain.reverse();
    chain
}

fn resolve_rewind_target(
    path: &Path,
    message_id: &str,
) -> std::result::Result<ClaudeRewindTarget, DomainStateError> {
    let file_len = std::fs::metadata(path)
        .map_err(|error| {
            session_not_running(format!(
                "The session's Claude transcript could not be read: {error}"
            ))
        })?
        .len();
    let rows = read_claude_transcript_rows(path).map_err(|error| {
        session_not_running(format!(
            "The session's Claude transcript could not be read: {error}"
        ))
    })?;
    let leaf = active_leaf_id(path, &rows, file_len);
    let chain = active_conversation(&rows, leaf.as_deref());
    let prompts: Vec<&ClaudeTranscriptRow> = chain
        .into_iter()
        .filter(|row| row.prompt_text.is_some())
        .collect();
    let position = prompts
        .iter()
        .position(|row| row.lineage.id == message_id)
        .ok_or_else(|| {
            message_not_found(
                "That message is not a user prompt of this conversation, so there is nothing to rewind to."
                    .to_string(),
            )
        })?;
    let target = prompts[position];
    let text = target.prompt_text.clone().unwrap_or_default();
    let first_line = prompt_first_line(&text);
    if first_line.is_empty() {
        return Err(message_not_found(
            "That prompt has no text Claude Code can show in its rewind list, so it cannot be selected."
                .to_string(),
        ));
    }
    Ok(ClaudeRewindTarget {
        message_id: target.lineage.id.clone(),
        leaf_id: target.lineage.parent_id.clone(),
        first_line,
        prompts_after: prompts.len() - position - 1,
    })
}

// ---------------------------------------------------------------------------
// The job registry the send worker's step reads
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct ClaudeRewindPlan {
    /// First line of the target prompt, space-collapsed.
    target_first_line: String,
    /// Up presses from `(current)` to the target: one per prompt after it, plus
    /// one to leave `(current)`.
    presses: usize,
}

struct ClaudeRewindJob {
    plan: ClaudeRewindPlan,
    outcome: Option<std::result::Result<(), DomainStateError>>,
}

static REWIND_JOBS: OnceLock<Mutex<HashMap<u64, ClaudeRewindJob>>> = OnceLock::new();
static REWIND_JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn rewind_jobs() -> &'static Mutex<HashMap<u64, ClaudeRewindJob>> {
    REWIND_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_rewind_job(plan: ClaudeRewindPlan) -> u64 {
    let job_id = REWIND_JOB_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut jobs) = rewind_jobs().lock() {
        jobs.insert(
            job_id,
            ClaudeRewindJob {
                plan,
                outcome: None,
            },
        );
    }
    job_id
}

fn take_rewind_job_outcome(job_id: u64) -> Option<std::result::Result<(), DomainStateError>> {
    rewind_jobs()
        .lock()
        .ok()
        .and_then(|mut jobs| jobs.remove(&job_id))
        .and_then(|job| job.outcome)
}

/*
CDXC:SessionChatRewind 2026-09-02:
What Claude Code leaves in the composer after it accepts a rewind. Measured
live on 2026-09-02 (Claude Code 2.1.258, session `S90-P3lv0-G8fxj`): choosing
`Restore conversation` closes the dialog and PRE-FILLS the composer with the
prompt that was rewound away, ready to be edited and re-sent. Escape does not
clear it.

That is a good thing for a human and a problem for this driver, because the
next rewind starts by typing `/rewind` into that same composer. Refusing every
rewind that follows a rewind would make the feature one-shot, and clearing any
draft we find would delete text the user may have typed themselves. So the
drive REMEMBERS the exact draft its own rewind left behind: a composer still
holding precisely that text is Claude's own restored prompt, untouched, and is
cleared with the standard burst before the next `/rewind`. One keystroke of
user editing changes the text and the drive refuses instead.
*/
static RESTORED_DRAFTS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn restored_drafts() -> &'static Mutex<HashMap<String, String>> {
    RESTORED_DRAFTS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_restored_draft(project_id: &str, session_id: &str, draft: &str) {
    let key = format!("{project_id}|{session_id}");
    if let Ok(mut drafts) = restored_drafts().lock() {
        if draft.is_empty() {
            drafts.remove(&key);
        } else {
            drafts.insert(key, draft.to_string());
        }
    }
}

fn is_restored_draft(project_id: &str, session_id: &str, draft: &str) -> bool {
    let key = format!("{project_id}|{session_id}");
    restored_drafts()
        .lock()
        .ok()
        .and_then(|drafts| drafts.get(&key).cloned())
        .is_some_and(|remembered| remembered == draft)
}

/// One rewind at a time per session. A second call while a drive is in flight
/// would queue its keystrokes behind the first one's and type them into
/// whatever the first one left on screen.
static REWINDS_IN_FLIGHT: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

struct RewindInFlightGuard {
    key: String,
}

impl RewindInFlightGuard {
    fn claim(project_id: &str, session_id: &str) -> Option<Self> {
        let key = format!("{project_id}|{session_id}");
        let mut in_flight = REWINDS_IN_FLIGHT
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .ok()?;
        if in_flight.iter().any(|entry| *entry == key) {
            return None;
        }
        in_flight.push(key.clone());
        Some(Self { key })
    }
}

impl Drop for RewindInFlightGuard {
    fn drop(&mut self) {
        if let Ok(mut in_flight) = REWINDS_IN_FLIGHT
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
        {
            in_flight.retain(|entry| *entry != self.key);
        }
    }
}

// ---------------------------------------------------------------------------
// The driver, run by the per-session send worker
// ---------------------------------------------------------------------------

struct RewindDriver<'a> {
    project_id: &'a str,
    session_id: &'a str,
    zmx_name: &'a str,
    source: &'a str,
    cancelled: &'a (dyn Fn() -> bool + Send + Sync),
}

impl RewindDriver<'_> {
    async fn write(&self, payload: &str) -> std::result::Result<(), DomainStateError> {
        write_session_chat_payload(
            self.project_id,
            self.session_id,
            self.zmx_name,
            self.source,
            payload,
        )
        .await
        .map_err(|error| session_not_running(format!("{error} The rewind was not started.")))
    }

    async fn capture(&self) -> Option<String> {
        capture_session_terminal_text(self.zmx_name).await
    }

    /// Polls the screen until `accept` answers `Some`, the step deadline
    /// passes, or the session's send generation is superseded.
    async fn wait_for<T>(
        &self,
        step: &str,
        mut accept: impl FnMut(&str) -> Option<T>,
    ) -> std::result::Result<T, DomainStateError> {
        let deadline = std::time::Instant::now() + Duration::from_millis(REWIND_STEP_TIMEOUT_MS);
        loop {
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The rewind was cancelled by another action on this session.".to_string(),
                ));
            }
            if let Some(screen) = self.capture().await {
                if let Some(value) = accept(&screen) {
                    return Ok(value);
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(rewind_timeout(step));
            }
            tokio::time::sleep(Duration::from_millis(REWIND_POLL_MS)).await;
        }
    }

    /// Polls a stage of the dialog after a move key until `read` reports a
    /// value different from `previous`, and answers `(value, false)` when the
    /// highlight is still where it was after `REWIND_MOVE_SETTLE_MS`. Only a
    /// stage that never appears at all is an error: the dialog is gone or is
    /// showing something this driver does not recognise.
    async fn wait_for_move<T: PartialEq>(
        &self,
        step: &str,
        subtitle: &str,
        previous: &T,
        read: impl Fn(&[String]) -> Option<T>,
    ) -> std::result::Result<(T, bool), DomainStateError> {
        let started = std::time::Instant::now();
        let settle = Duration::from_millis(REWIND_MOVE_SETTLE_MS);
        let deadline = Duration::from_millis(REWIND_STEP_TIMEOUT_MS);
        let mut stayed: Option<T> = None;
        loop {
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The rewind was cancelled by another action on this session.".to_string(),
                ));
            }
            if let Some(screen) = self.capture().await {
                let lines = screen_lines(&screen);
                if let Some(value) =
                    rewind_stage_region(&lines, subtitle).and_then(|region| read(region))
                {
                    if value != *previous {
                        return Ok((value, true));
                    }
                    stayed = Some(value);
                }
            }
            if stayed.is_some() && started.elapsed() >= settle {
                return Ok((
                    stayed.expect("the settle branch only runs with a value"),
                    false,
                ));
            }
            if started.elapsed() >= deadline {
                return Err(rewind_timeout(step));
            }
            tokio::time::sleep(Duration::from_millis(REWIND_POLL_MS)).await;
        }
    }

    /// Close whatever the drive left on screen. Escape backs the confirmation
    /// stage out to the list and the list out to the composer, so two are
    /// enough, and each one is only written while a dialog is still up.
    async fn cancel_dialog(&self) {
        for _ in 0..REWIND_CANCEL_ESCAPES {
            let still_open = self
                .capture()
                .await
                .is_some_and(|screen| rewind_dialog_region(&screen_lines(&screen)).is_some());
            if !still_open {
                return;
            }
            if self.write(SESSION_CHAT_INTERRUPT).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(REWIND_COMMAND_SETTLE_MS)).await;
        }
    }

    async fn run(&self, plan: &ClaudeRewindPlan) -> std::result::Result<(), DomainStateError> {
        match self.drive(plan).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.cancel_dialog().await;
                Err(error)
            }
        }
    }

    async fn drive(&self, plan: &ClaudeRewindPlan) -> std::result::Result<(), DomainStateError> {
        /*
        The composer check is repeated HERE, not just in the handler, because
        the handler ran before this job reached the front of the session's send
        queue. What matters is the state of the screen one instant before the
        first keystroke.
        */
        let screen = self.capture().await.ok_or_else(|| {
            session_not_running(
                "The session's screen could not be read, so the rewind was not started."
                    .to_string(),
            )
        })?;
        match composer_draft(&screen) {
            Some(draft) if draft.is_empty() => {}
            Some(draft) if is_restored_draft(self.project_id, self.session_id, &draft) => {
                self.clear_restored_draft(&draft).await?;
            }
            Some(_) => {
                return Err(agent_busy(
                    "The terminal composer holds unsent text, so the rewind was not started. Send it or clear it in the terminal first."
                        .to_string(),
                ))
            }
            None => {
                return Err(agent_busy(
                    "Claude Code is not showing its input box, so the rewind was not started."
                        .to_string(),
                ))
            }
        }

        self.write(CLAUDE_REWIND_COMMAND).await?;
        tokio::time::sleep(Duration::from_millis(REWIND_COMMAND_SETTLE_MS)).await;
        self.write(CLAUDE_REWIND_SUBMIT).await?;
        self.wait_for("open", |screen| {
            rewind_stage_region(&screen_lines(screen), CLAUDE_REWIND_LIST_SUBTITLE).map(|_| ())
        })
        .await?;

        self.select_prompt(plan).await?;
        self.write(CLAUDE_REWIND_SUBMIT).await?;
        self.confirm(plan).await?;
        self.write(CLAUDE_REWIND_SUBMIT).await?;

        /*
        The drive is only done once the dialog is gone AND the composer is back:
        a dialog that is still up means the final Enter did something other than
        accept the option, and the caller must not be told the conversation
        moved. The draft Claude leaves in that composer is the prompt it just
        rewound away, and remembering it is what lets the NEXT rewind on this
        session clear it instead of refusing (see RESTORED_DRAFTS above).
        */
        let restored = self
            .wait_for("close", |screen| {
                let closed = rewind_dialog_region(&screen_lines(screen)).is_none();
                closed.then(|| composer_draft(screen)).flatten()
            })
            .await?;
        remember_restored_draft(self.project_id, self.session_id, &restored);
        Ok(())
    }

    /// Clear the prompt Claude restored into the composer after this driver's
    /// own previous rewind, with the measured burst every server-side writer of
    /// this input line uses, and prove the composer is empty before returning.
    async fn clear_restored_draft(&self, draft: &str) -> std::result::Result<(), DomainStateError> {
        self.write(&crate::session_chat_send::build_agent_tui_clear_input_for_text(draft))
            .await?;
        tokio::time::sleep(Duration::from_millis(
            crate::session_chat_send::SESSION_CHAT_CLEAR_INPUT_SETTLE_MS,
        ))
        .await;
        self.wait_for("composer", |screen| {
            composer_draft(screen)
                .filter(String::is_empty)
                .map(|_| ())
        })
        .await
        .map_err(|_| {
            agent_busy(
                "The prompt Claude Code restored into the terminal composer did not clear, so the rewind was not started."
                    .to_string(),
            )
        })?;
        remember_restored_draft(self.project_id, self.session_id, "");
        Ok(())
    }

    /// Walk the prompt list up to the target row. The counted number of presses
    /// is spent first, then the highlighted row must show the target prompt;
    /// a few extra presses are allowed for a list Claude counts differently,
    /// and anything past that is a refusal.
    async fn select_prompt(
        &self,
        plan: &ClaudeRewindPlan,
    ) -> std::result::Result<(), DomainStateError> {
        let mut last_row = self
            .wait_for("list", |screen| {
                rewind_stage_region(&screen_lines(screen), CLAUDE_REWIND_LIST_SUBTITLE)
                    .and_then(highlighted_row)
            })
            .await?;
        let limit = plan.presses.saturating_add(REWIND_EXTRA_PRESSES);
        for press in 1..=limit {
            self.write(CLAUDE_REWIND_LIST_UP).await?;
            let (row, moved) = self
                .wait_for_move(
                    "list",
                    CLAUDE_REWIND_LIST_SUBTITLE,
                    &last_row,
                    highlighted_row,
                )
                .await?;
            last_row = row;
            if press >= plan.presses && row_matches_prompt(&last_row, &plan.target_first_line) {
                return Ok(());
            }
            if !moved {
                return Err(dialog_mismatch(
                    "prompt list",
                    &format!(
                        "the list stopped on \"{last_row}\" without ever showing the message that was asked for."
                    ),
                ));
            }
        }
        Err(dialog_mismatch(
            "prompt list",
            &format!(
                "the highlighted prompt was \"{last_row}\" after {limit} moves, not the message that was asked for."
            ),
        ))
    }

    /// Verify the confirmation dialog is quoting the target prompt, then move
    /// the menu cursor onto the option labelled exactly `Restore conversation`.
    async fn confirm(&self, plan: &ClaudeRewindPlan) -> std::result::Result<(), DomainStateError> {
        let quoted = self
            .wait_for("confirmation", |screen| {
                let lines = screen_lines(screen);
                let region = rewind_stage_region(&lines, CLAUDE_REWIND_CONFIRM_SUBTITLE)?;
                confirm_quoted_first_line(region)
            })
            .await?;
        if !row_matches_prompt(&quoted, &plan.target_first_line) {
            return Err(dialog_mismatch(
                "confirmation",
                &format!("it is quoting \"{quoted}\", not the message that was asked for."),
            ));
        }
        for _ in 0..REWIND_MENU_MAX_PRESSES {
            let options = self
                .wait_for("confirmation", |screen| {
                    let lines = screen_lines(screen);
                    let region = rewind_stage_region(&lines, CLAUDE_REWIND_CONFIRM_SUBTITLE)?;
                    let options = parse_menu_options(region);
                    options
                        .iter()
                        .any(|option| option.selected)
                        .then_some(options)
                })
                .await?;
            let selected = options
                .iter()
                .find(|option| option.selected)
                .expect("the wait only returns a menu that has a selected option");
            if selected.label == CLAUDE_REWIND_RESTORE_CONVERSATION {
                return Ok(());
            }
            /*
            `Restore conversation` sits at or below the option the menu opens
            on, in both measured shapes (option 1 when the turn changed no
            files, option 2 under `Restore code and conversation` when it did).
            A cursor already past it means this is a menu shape nobody
            measured, and guessing there is how `Restore code and conversation`
            gets picked and a user's working tree gets reverted.
            */
            if let Some(target) = options
                .iter()
                .find(|option| option.label == CLAUDE_REWIND_RESTORE_CONVERSATION)
            {
                if target.number < selected.number {
                    return Err(dialog_mismatch(
                        "confirmation",
                        &format!(
                            "the cursor is on option {} and \"{CLAUDE_REWIND_RESTORE_CONVERSATION}\" is option {} above it."
                            , selected.number, target.number
                        ),
                    ));
                }
            }
            let selected_number = selected.number;
            self.write(CLAUDE_REWIND_MENU_DOWN).await?;
            let (_, moved) = self
                .wait_for_move(
                    "confirmation",
                    CLAUDE_REWIND_CONFIRM_SUBTITLE,
                    &selected_number,
                    |region| {
                        parse_menu_options(region)
                            .into_iter()
                            .find(|option| option.selected)
                            .map(|option| option.number)
                    },
                )
                .await?;
            if !moved {
                return Err(dialog_mismatch(
                    "confirmation",
                    &format!(
                        "the cursor stayed on option {selected_number} instead of reaching \"{CLAUDE_REWIND_RESTORE_CONVERSATION}\"."
                    ),
                ));
            }
        }
        Err(dialog_mismatch(
            "confirmation",
            &format!(
                "\"{CLAUDE_REWIND_RESTORE_CONVERSATION}\" was not reachable in the option menu."
            ),
        ))
    }
}

/// The `SessionChatSendStep::DriveClaudeRewind` body. Runs on the session's own
/// send worker, so nothing else can write to that pty while the dialog is open.
pub(crate) async fn run_claude_rewind_job(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    job_id: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) {
    let plan = rewind_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_id).map(|job| job.plan.clone()));
    let Some(plan) = plan else {
        return;
    };
    let driver = RewindDriver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = driver.run(&plan).await;
    if let Err(error) = outcome.as_ref() {
        log_session_chat_rewind(
            LogLevel::Error,
            "sessionChatRewindFailed",
            json!({
                "projectId": project_id,
                "providerSessionId": zmx_name,
                "sessionId": session_id,
                "code": error.code,
                "presses": plan.presses,
            }),
            Some(error.message.clone()),
        );
    }
    if let Ok(mut jobs) = rewind_jobs().lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            job.outcome = Some(outcome);
        }
    }
}

// ---------------------------------------------------------------------------
// /api/rewindSessionChat
// ---------------------------------------------------------------------------

pub(crate) async fn handle_rewind_session_chat_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    match rewind_session_chat(state, &params).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

async fn rewind_session_chat(
    state: &AppState,
    params: &Map<String, Value>,
) -> std::result::Result<Value, DomainStateError> {
    let message_id = params
        .get("messageId")
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string();
    if message_id.is_empty() {
        return Err(DomainStateError {
            code: "invalidParams",
            message: "rewindSessionChat requires messageId.".to_string(),
        });
    }
    let target = resolve_session_chat_send_target(state, params, "rewindSessionChat")?;
    let agent = crate::session_chat_follower::session_chat_agent_for_session(&target.session);
    if !matches!(agent.as_deref(), Some("claude" | "openclaude")) {
        return Err(unsupported_agent());
    }
    if crate::presentation::effective_lifecycle_state(&target.session) != "running" {
        return Err(session_not_running(
            "The session is not running, so its agent has no rewind dialog to drive.".to_string(),
        ));
    }
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if crate::presentation::presentation_activity(&target.session, &generated_at) == "working" {
        return Err(agent_busy(
            "Claude Code is still working on a turn. Wait for it to finish, or stop it, and then rewind."
                .to_string(),
        ));
    }
    let transcript_path = claude_transcript_path(&target.session)?;
    let rewind_target = resolve_rewind_target(&transcript_path, &message_id)?;

    let Some(_guard) = RewindInFlightGuard::claim(&target.project_id, &target.session_id) else {
        return Err(agent_busy(
            "A rewind is already running for this session.".to_string(),
        ));
    };
    let job_id = register_rewind_job(ClaudeRewindPlan {
        target_first_line: rewind_target.first_line.clone(),
        presses: rewind_target.prompts_after.saturating_add(1),
    });
    let send = execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-rewind",
        vec![SessionChatSendStep::DriveClaudeRewind { job_id }],
    )
    .await;
    let outcome = take_rewind_job_outcome(job_id);
    match (send, outcome) {
        (_, Some(Err(error))) => return Err(error),
        (Err(error), _) => {
            return Err(agent_busy(format!(
                "{} The rewind did not run.",
                error.message
            )))
        }
        (Ok(()), None) => {
            return Err(agent_busy(
                "The session's terminal queue dropped the rewind before it ran.".to_string(),
            ))
        }
        (Ok(()), Some(Ok(()))) => {}
    }

    /*
    The TUI has accepted the rewind, and the transcript still shows every row
    the conversation just left behind. Recording the leaf here is what lets the
    chat readers hide them on the very next read instead of waiting for the
    next prompt to prove the branch.
    */
    let cutoff_offset = std::fs::metadata(&transcript_path)
        .map(|metadata| metadata.len())
        .unwrap_or_default();
    set_session_chat_pending_rewind(
        &transcript_path,
        SessionChatPendingRewind {
            leaf_id: rewind_target.leaf_id.clone(),
            cutoff_offset,
            set_at_ms: chrono::Utc::now().timestamp_millis(),
        },
    );
    crate::session_chat_follower::request_session_chat_resnapshot(
        state,
        &target.project_id,
        &target.session_id,
    );
    log_session_chat_rewind(
        LogLevel::Info,
        "sessionChatRewound",
        json!({
            "projectId": target.project_id,
            "sessionId": target.session_id,
            "targetMessageId": rewind_target.message_id,
            "leafId": rewind_target.leaf_id,
            "cutoffOffset": cutoff_offset,
        }),
        None,
    );
    Ok(json!({
        "ok": true,
        "targetMessageId": rewind_target.message_id,
        "leafId": rewind_target.leaf_id,
    }))
}

fn claude_transcript_path(session: &Value) -> std::result::Result<PathBuf, DomainStateError> {
    crate::session_chat::resolve_session_chat_transcript_path(
        SessionChatTranscriptAgent::Claude,
        read_runtime_text(session, "agentSessionId").as_deref(),
        read_runtime_text(session, "agentSessionPath").as_deref(),
    )
    .ok_or_else(|| {
        message_not_found(
            "This session has no Claude transcript yet, so there is nothing to rewind to."
                .to_string(),
        )
    })
}
