/*
CDXC:AgentScreenDetection 2026-09-03 DECISION:
User: "i want codex to list the available models and efforts and if fast is enabled/disabled (like claude does now)", and the model must be changeable from the chat box, not only from the terminal.

CDXC:AgentScreenDetection 2026-09-03 WHY:
Codex has no non-interactive way to set the model. `/model <name>` is not a command: the CLI sends it to the model as a prompt and starts a turn (measured on Codex 0.153). The only channel is its own `/model` picker: a numbered "Select Model and Effort" list, then a numbered "Select Reasoning Level for <model>" list, where a digit both selects and confirms a row. So `/api/selectSessionChatModel` drives that picker in the session's terminal the way the Claude rewind driver drives `/rewind`: every keystroke is preceded by a screen capture that must show the expected list, the digit is read off the row that names the requested model (never computed from a catalog index), and any capture that disagrees aborts with Escape instead of typing into whatever is on screen. The whole drive is one job on the session's serialized send worker, so a queued prompt can never land inside the picker.
SEE-ALSO: packages/gx-chat-core/src/menus/option_catalog.rs (the `model-picker` dispatch), session_chat_rewind.rs (the driver pattern).
*/

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

#[path = "session_chat_provider_model_picker.rs"]
mod provider_model_picker;
use provider_model_picker::empryo_picker;
pub(crate) use provider_model_picker::run_provider_model_picker_job;
#[path = "session_chat_claude_effort_slider.rs"]
mod claude_effort_slider;
#[path = "session_chat_claude_model_list.rs"]
mod claude_model_list;
#[path = "session_chat_codex_permissions.rs"]
mod codex_permissions;
#[path = "session_chat_selection_options.rs"]
mod selection_options;
use claude_model_list::{
    claude_model_picker_open, screen_lines_spaced, CLAUDE_ARROW_RIGHT, CLAUDE_EFFORT_STEP_LIMIT,
    CLAUDE_PICKER_TAIL_LINES, CLAUDE_SESSION_ONLY_KEY,
};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::session_chat_options::{
    detect_session_chat_selection, SessionChatOptionAgent, CODEX_EFFORTS,
};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, resolve_session_chat_send_target,
    write_session_chat_payload, SessionChatSendStep, SESSION_CHAT_INTERRUPT,
    SESSION_CHAT_SHIFT_DOWN, SESSION_CHAT_SHIFT_UP,
};

/// Typed as literal keystrokes so the composer's slash popup runs it.
const CODEX_MODEL_COMMAND: &str = "/model";
const CODEX_SUBMIT: &str = "\r";

const CODEX_MODEL_PICKER_TITLE: &str = "Select Model and Effort";
const CODEX_EFFORT_PICKER_TITLE_PREFIX: &str = "Select Reasoning Level for ";
/// Row that opens the Max / Ultra sub-list.
const CODEX_MORE_REASONING_ROW: &str = "More reasoning";
const CODEX_ADVANCED_REASONING_TITLE: &str = "Advanced Reasoning";
/// The line Codex prints once the picker has applied the change.
const CODEX_MODEL_CHANGED_PREFIX: &str = "Model changed to ";
/// Highlight marker on the picker's current row (and Codex's composer prompt).
const CODEX_CURSOR: char = '\u{203a}';
/// The reasoning lists' footer names this key since Codex 0.157 (`enter default · s session · esc back`).
const CODEX_SESSION_KEY_HINT: &str = "s session";
const CODEX_SESSION_KEY: &str = "s";
/// CDXC:AgentScreenDetection 2026-09-28 WHY:
/// What Codex adds to its change line when `s` applied the pick to this conversation alone. Codex 0.157 prints `for this session only` (`tui/src/app/model_defaults.rs`) while its conversation-default path still prints `for this conversation`; accepting only the older wording timed out every session-scoped pick after the change had already been applied.
const CODEX_SESSION_CHANGED_SUFFIXES: [&str; 2] =
    ["for this session only", "for this conversation"];
const CODEX_ARROW_UP: &str = "\u{1b}[A";
const CODEX_ARROW_DOWN: &str = "\u{1b}[B";
/// One press per row of the longest reasoning list, with slack.
const CODEX_ROW_STEP_LIMIT: usize = 12;
const CODEX_ULTRA_CURSOR: char = '\u{00bb}';

/// Typed as a bracketed paste; its list lives in session_chat_claude_model_list.rs.
const CLAUDE_MODEL_COMMAND: &str = "/model";

/// CDXC:SessionChat 2026-09-27 WHY:
/// A TUI repaints a keystroke within a frame or two (Codex's picker answered in under 50ms when measured), and a screen capture costs about 3ms, so a step is checked every 15ms at first. Polling every 150ms made each of a pick's four to eight steps wait a whole interval and a Codex switch took 1.3-1.8s while Codex itself needed about 200ms. A step still waiting after the fast window is waiting on the agent and backs off.
const PICKER_FAST_POLL_MS: u64 = 15;
const PICKER_FAST_POLL_WINDOW_MS: u64 = 1_500;
const PICKER_POLL_MS: u64 = 100;
const PICKER_STEP_TIMEOUT_MS: u64 = 6_000;
/// Error recovery gives the dialog time to close before checking its parent.
const PICKER_CANCEL_SETTLE_MS: u64 = 300;
/// Advanced reasoning backs out through effort and model to the composer.
const PICKER_CANCEL_ESCAPES: usize = 3;

static LOGGER: OnceLock<GxserverLogger> = OnceLock::new();

fn log_picker(level: LogLevel, event: &str, details: Value, error: Option<String>) {
    let logger = LOGGER.get_or_init(|| GxserverLogger::new(crate::paths::get_gxserver_paths(None)));
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

fn invalid_params(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "invalidParams",
        message: message.into(),
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

fn dialog_mismatch(step: &str, detail: &str) -> DomainStateError {
    DomainStateError {
        code: "dialogMismatch",
        message: format!(
            "Codex's model picker did not show what was expected at the {step} step: {detail}"
        ),
    }
}

/// CDXC:SessionChat 2026-09-18 WHY:
/// A model the terminal's own list does not offer can never be applied to one session, so the
/// durable queue drops it instead of retyping `/model` into the user's pane every few seconds.
/// One catalog entry that Claude Code does not list for an account retried 181 times before this.
/// SEE-ALSO: server/src/session_chat_model_selection.rs sender treats this code as final.
pub(crate) const UNSUPPORTED_SELECTION_CODE: &str = "unsupportedSelection";

pub(crate) fn unsupported_selection(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: UNSUPPORTED_SELECTION_CODE,
        message: message.into(),
    }
}

/// What a pick fails with while the agent's input box is not drawn at all; the queue gives up on
/// it after a while (session_chat_model_selection.rs `MISSING_INPUT_BOX_LIMIT_MS`).
pub(crate) const INPUT_BOX_MISSING_CODE: &str = "composerNotReady";

/// The refusal for a screen without a usable input box: `composerNotReady` when nothing of the
/// agent is drawn (CDXC:SessionChat 2026-10-24 in session_chat_composer.rs), `agentBusy` when a
/// dialog or panel stands in its place.
fn input_box_refusal(
    readiness: &crate::session_chat_composer::SessionChatComposerReadiness,
    message: &str,
) -> DomainStateError {
    DomainStateError {
        code: if readiness.input_box_missing() {
            INPUT_BOX_MISSING_CODE
        } else {
            "agentBusy"
        },
        message: message.to_string(),
    }
}

fn picker_timeout(step: &str) -> DomainStateError {
    DomainStateError {
        code: "timeout",
        message: format!("The agent did not confirm the model picker's {step} step in time."),
    }
}

// ---------------------------------------------------------------------------
// Screen parsing
// ---------------------------------------------------------------------------

fn collapse_spaces(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn screen_lines(screen: &str) -> Vec<String> {
    screen
        .split('\n')
        .map(|line| collapse_spaces(&crate::session_chat_options::strip_ansi_sgr(line)))
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PickerRow {
    number: u32,
    /// The row text after `N.`, with the highlight marker removed.
    text: String,
    /// Whether the highlight marker sits on this row.
    selected: bool,
}

/// `› 2. gpt-5.6-terra (current) Balanced agentic …` → number 2, text after the dot.
fn parse_picker_row(line: &str) -> Option<PickerRow> {
    let selected = line.starts_with(CODEX_CURSOR);
    let rest = line
        .strip_prefix(CODEX_CURSOR)
        .map(str::trim_start)
        .unwrap_or(line);
    let dot = rest.find(". ")?;
    let number = rest[..dot].parse::<u32>().ok()?;
    let text = rest[dot + 2..].trim();
    (!text.is_empty()).then(|| PickerRow {
        number,
        text: text.to_string(),
        selected,
    })
}

/// The numbered rows below the LAST line equal to `title` (or starting with
/// it, for the effort list whose title names the model). Taking the last title
/// keeps an earlier, already-answered picker in scrollback out of the parse.
fn picker_rows_under(
    lines: &[String],
    title_matches: impl Fn(&str) -> bool,
) -> Option<Vec<PickerRow>> {
    let title = lines.iter().rposition(|line| title_matches(line))?;
    let rows: Vec<PickerRow> = lines[title + 1..]
        .iter()
        .filter_map(|line| parse_picker_row(line))
        .collect();
    (!rows.is_empty()).then_some(rows)
}

fn model_picker_rows(screen: &str) -> Option<Vec<PickerRow>> {
    picker_rows_under(&screen_lines(screen), |line| {
        line == CODEX_MODEL_PICKER_TITLE
    })
}

/// Whether `line` is the reasoning list's title for `model`.
fn effort_picker_title_names(line: &str, model: &str) -> bool {
    line.strip_prefix(CODEX_EFFORT_PICKER_TITLE_PREFIX)
        .is_some_and(|name| codex_name_matches(name, model))
}

fn effort_picker_rows(screen: &str, model: &str) -> Option<Vec<PickerRow>> {
    picker_rows_under(&screen_lines(screen), |line| {
        effort_picker_title_names(line, model)
    })
}

/// Whether the reasoning list for `model` offers Codex's session key; builds before 0.157 do not.
fn effort_picker_offers_session(screen: &str, model: &str) -> bool {
    let lines = screen_lines(screen);
    lines
        .iter()
        .rposition(|line| effort_picker_title_names(line, model))
        .is_some_and(|title| {
            lines[title + 1..]
                .iter()
                .any(|line| line.contains(CODEX_SESSION_KEY_HINT))
        })
}

fn advanced_effort_picker_rows(screen: &str) -> Option<Vec<PickerRow>> {
    picker_rows_under(&screen_lines(screen), |line| {
        line == CODEX_ADVANCED_REASONING_TITLE
    })
}

/// Claude's "Switch model?" or "Switch effort?" warning ("This conversation is cached for the
/// current model…"), answered with its "Yes, switch to …" row.
fn claude_switch_warning_answer(screen: &str) -> Option<String> {
    use crate::session_chat_resume_prompt::{
        detect_session_chat_terminal_picker, SessionChatTerminalPickerKind,
    };
    let picker = detect_session_chat_terminal_picker(screen)?;
    if !matches!(
        picker.kind,
        SessionChatTerminalPickerKind::SwitchModel | SessionChatTerminalPickerKind::SwitchEffort
    ) {
        return None;
    }
    picker
        .rows
        .iter()
        .position(|row| row.label.starts_with("Yes, switch to "))
        .and_then(|index| picker.answer_key(index))
}

fn any_picker_open(screen: &str) -> bool {
    claude_switch_warning_answer(screen).is_some()
        || claude_model_picker_open(screen)
        || claude_effort_slider::claude_effort_slider_open(screen)
        || screen_lines(screen).iter().any(|line| {
            line == CODEX_MODEL_PICKER_TITLE
                || line.starts_with(CODEX_EFFORT_PICKER_TITLE_PREFIX)
                || line == CODEX_ADVANCED_REASONING_TITLE
        })
}

/// Whether a row's text is the model or effort label `wanted`: the label is
/// the whole text, or the text continues with a space (` (current)`, ` (default)`,
/// or the row's description).
///
/// CDXC:AgentScreenDetection 2026-09-24 WHY:
/// Codex 0.156 lists display names (`GPT-6-Sol`) in its model picker and effort title while the
/// catalog, the footer reader and the "Model changed to" line use ids (`gpt-6-sol`), so the label
/// is compared without case. An exact match failed every Codex model change from chat.
/// SEE-ALSO: server/src/session_chat_options/agent_matchers.rs codex_model_value.
fn row_names(row: &PickerRow, wanted: &str) -> bool {
    row.text
        .get(..wanted.len())
        .is_some_and(|head| codex_name_matches(head, wanted))
        && row.text[wanted.len()..]
            .chars()
            .next()
            .is_none_or(|next| next == ' ')
}

/// Whether a name Codex painted is `wanted`, ignoring case and reading a space as the id's hyphen.
///
/// CDXC:AgentScreenDetection 2026-10-28 WHY: Codex 0.164 paints GPT-6.1 Terra as "GPT-6.1 Terra"
/// in its model list and reasoning title, a space where every other display name has the id's
/// hyphen (`gpt-6.1-terra`), so choosing Terra from the chat failed with "Codex's model picker
/// lists … GPT-6.1 …, not gpt-6.1-terra" (found testing Codex on Linux; the picker is shared by
/// every platform). SEE-ALSO: session_chat_options/agent_matchers.rs codex_footer_model.
fn codex_name_matches(painted: &str, wanted: &str) -> bool {
    painted.len() == wanted.len()
        && painted
            .bytes()
            .zip(wanted.bytes())
            .all(|(painted, wanted)| {
                painted.eq_ignore_ascii_case(&wanted) || (painted == b' ' && wanted == b'-')
            })
}

/// The label Codex paints for a catalog effort id.
fn effort_row_label(effort: &str) -> Option<&'static str> {
    Some(match effort {
        "minimal" => "Minimal",
        "low" => "Low",
        "medium" => "Medium",
        "high" => "High",
        "xhigh" => "Extra high",
        "max" => "Max",
        "ultra" => "Ultra",
        _ => return None,
    })
}

/// The keystroke that selects and confirms `row`. Codex's digit shortcut only
/// covers single-digit rows, and no list Ghostex drives is longer than nine.
fn row_key(row: &PickerRow, step: &str) -> Result<String, DomainStateError> {
    if (1..=9).contains(&row.number) {
        Ok(row.number.to_string())
    } else {
        Err(dialog_mismatch(
            step,
            &format!(
                "row {} is past the single-digit shortcuts, so it cannot be selected.",
                row.number
            ),
        ))
    }
}

/// `• Model changed to gpt-6-luna low`, or with ` for this conversation` after a session pick,
/// which is the only proof that `s` applied it without saving the default.
fn changed_line_present(screen: &str, model: &str, effort: &str, session_only: bool) -> bool {
    let expected = format!("{CODEX_MODEL_CHANGED_PREFIX}{model} {effort}");
    screen_lines(screen).iter().any(|line| {
        let rest = line
            .strip_prefix(CODEX_CURSOR)
            .map(str::trim_start)
            .unwrap_or(line);
        rest.trim_start_matches(['•', ' '])
            .strip_prefix(&expected)
            .is_some_and(|tail| {
                !session_only || CODEX_SESSION_CHANGED_SUFFIXES.contains(&tail.trim())
            })
    })
}

// ---------------------------------------------------------------------------
// Job registry the send worker's step reads
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, Default)]
struct CodexPickerPlan {
    provider: String,
    model: String,
    effort: String,
    options: crate::session_chat_model_selection::SelectionOptions,
    /// Apply the pick to this session without saving the agent's default. Claude only.
    session_only: bool,
    claude_statusline: Option<(std::path::PathBuf, String)>,
    /// Hermes only: the `--provider` a model from another provider needs.
    hermes_provider: Option<String>,
    /// Empryo only: the session's `session.jsonl`, whose `meta.json` names the tab's model.
    empryo_session_log: Option<std::path::PathBuf>,
}

struct CodexPickerJob {
    plan: CodexPickerPlan,
    outcome: Option<Result<(), DomainStateError>>,
}

static PICKER_JOBS: OnceLock<Mutex<HashMap<u64, CodexPickerJob>>> = OnceLock::new();
static PICKER_JOB_SEQUENCE: AtomicU64 = AtomicU64::new(1);

fn picker_jobs() -> &'static Mutex<HashMap<u64, CodexPickerJob>> {
    PICKER_JOBS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn register_job(plan: CodexPickerPlan) -> u64 {
    let job_id = PICKER_JOB_SEQUENCE.fetch_add(1, Ordering::SeqCst);
    if let Ok(mut jobs) = picker_jobs().lock() {
        jobs.insert(
            job_id,
            CodexPickerJob {
                plan,
                outcome: None,
            },
        );
    }
    job_id
}

fn take_job_outcome(job_id: u64) -> Option<Result<(), DomainStateError>> {
    picker_jobs()
        .lock()
        .ok()
        .and_then(|mut jobs| jobs.remove(&job_id))
        .and_then(|job| job.outcome)
}

static PICKS_IN_FLIGHT: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

struct PickInFlightGuard {
    key: String,
}

impl PickInFlightGuard {
    fn claim(project_id: &str, session_id: &str) -> Option<Self> {
        let key = format!("{project_id}\u{0}{session_id}");
        let mut in_flight = PICKS_IN_FLIGHT
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .ok()?;
        if in_flight.contains(&key) {
            return None;
        }
        in_flight.push(key.clone());
        Some(Self { key })
    }
}

impl Drop for PickInFlightGuard {
    fn drop(&mut self) {
        if let Some(mut in_flight) = PICKS_IN_FLIGHT.get().and_then(|lock| lock.lock().ok()) {
            in_flight.retain(|entry| entry != &self.key);
        }
    }
}

// ---------------------------------------------------------------------------
// The driver
// ---------------------------------------------------------------------------

fn claude_model_label_matches(label: &str, value: &str) -> bool {
    let target = label
        .to_ascii_lowercase()
        .replace(" (1m context)", "[1m]")
        .replace(" (1m)", "[1m]")
        .replace([' ', '.'], "-");
    // "Fable 5.2" reads as `fable-5-2`: a dotted version token stopped the version scan, so
    // the row and Claude's "Set model to Fable 5.2" reply named the `fable` alias (Fable 5.1).
    target == value
        || crate::session_chat_options::claude_transcript_model_choice(&target)
            .is_some_and(|choice| choice.value == value)
}

fn claude_switch_confirmation(
    screen: &str,
    field: &str,
    value: &str,
) -> Option<crate::session_chat_resume_prompt::SessionChatTerminalPicker> {
    use crate::session_chat_resume_prompt::{
        detect_session_chat_terminal_picker, SessionChatTerminalPickerKind,
    };
    let picker = detect_session_chat_terminal_picker(screen)?;
    let expected = if field == "model" {
        SessionChatTerminalPickerKind::SwitchModel
    } else {
        SessionChatTerminalPickerKind::SwitchEffort
    };
    if picker.kind != expected {
        return None;
    }
    let target = picker
        .rows
        .iter()
        .find_map(|row| row.label.strip_prefix("Yes, switch to "))?;
    let matches = if field == "effort" {
        target.eq_ignore_ascii_case(value)
    } else {
        claude_model_label_matches(target, value)
    };
    matches.then_some(picker)
}

fn claude_option_applied(screen: &str, field: &str, value: &str) -> bool {
    if crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("claude"),
        screen,
        None,
    )
    .state
        != crate::session_chat_composer::SessionChatComposerState::Ready
    {
        return false;
    }
    let lines = screen_lines(screen);
    let command = format!("❯ /{field} {value}");
    let Some(command_index) = lines.iter().rposition(|line| line == &command) else {
        return false;
    };
    let Some(reply) = lines[command_index + 1..]
        .iter()
        .find(|line| !line.is_empty())
    else {
        return false;
    };
    if field == "effort" {
        reply
            .strip_prefix("⎿ Set effort level to ")
            .and_then(|rest| rest.split_whitespace().next())
            .is_some_and(|actual| actual == value)
    } else {
        reply
            .strip_prefix("⎿ Set model to ")
            .map(|rest| rest.split(" and saved").next().unwrap_or(rest))
            .is_some_and(|label| claude_model_label_matches(label, value))
    }
}

struct PickerDriver<'a> {
    project_id: &'a str,
    session_id: &'a str,
    zmx_name: &'a str,
    source: &'a str,
    cancelled: &'a (dyn Fn() -> bool + Send + Sync),
}

impl PickerDriver<'_> {
    async fn write(&self, payload: &str) -> Result<(), DomainStateError> {
        write_session_chat_payload(
            self.project_id,
            self.session_id,
            self.zmx_name,
            self.source,
            payload,
        )
        .await
        .map_err(|error| session_not_running(format!("{error} The model was not changed.")))
    }

    async fn capture(&self) -> Option<String> {
        capture_session_terminal_text(self.zmx_name).await
    }

    /// CDXC:SessionChat 2026-09-26 WHY:
    /// A plain capture cannot tell Claude's faint placeholder (`Try "fix lint errors"`) or its next-prompt suggestion from a typed draft, so every queued model or option change on an idle session was refused forever and held the chat messages queued behind it at "Waiting for agent…". Only a VT capture keeps the faint style that marks them as empty; an unreadable screen still counts as a draft so a real one is never replaced.
    async fn claude_input_holds_draft(&self) -> bool {
        crate::session_chat_send::capture_session_terminal_text_vt(self.zmx_name)
            .await
            .is_none_or(|screen| {
                crate::session_chat_composer::session_chat_composer_input("claude", &screen)
                    .is_some_and(|input| !input.is_empty())
            })
    }

    /// Polls the screen until `accept` answers `Some`, the step deadline
    /// passes, or the session's send generation is superseded.
    async fn wait_for<T>(
        &self,
        step: &str,
        accept: impl FnMut(&str) -> Option<T>,
    ) -> Result<T, DomainStateError> {
        self.wait_for_within(step, PICKER_STEP_TIMEOUT_MS, accept)
            .await
    }

    /// [`Self::wait_for`] with its own deadline, for a step the agent itself may spend seconds on.
    async fn wait_for_within<T>(
        &self,
        step: &str,
        timeout_ms: u64,
        mut accept: impl FnMut(&str) -> Option<T>,
    ) -> Result<T, DomainStateError> {
        let started = std::time::Instant::now();
        let deadline = started + Duration::from_millis(timeout_ms);
        loop {
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The model change was cancelled by another action on this session.",
                ));
            }
            if let Some(screen) = self.capture().await {
                if let Some(value) = accept(&screen) {
                    return Ok(value);
                }
            }
            if std::time::Instant::now() >= deadline {
                return Err(picker_timeout(step));
            }
            let poll_ms = if started.elapsed() < Duration::from_millis(PICKER_FAST_POLL_WINDOW_MS) {
                PICKER_FAST_POLL_MS
            } else {
                PICKER_POLL_MS
            };
            tokio::time::sleep(Duration::from_millis(poll_ms)).await;
        }
    }

    /// CDXC:SessionChat 2026-09-29 WHY:
    /// Once a conversation has history, Claude Code 2.1.284 answers a session-only model pick (`s` in
    /// the `/model` list) and an effort slider confirmation with "Switch model?" / "Switch effort?"
    /// ("Your next response will be slower and use more tokens"). Only the typed `/model <name>` path
    /// answered it, so every session-only pick in a real conversation timed out with the warning
    /// open and retried forever. The warning follows the pick this job just made, so it is answered
    /// with its "Yes, switch to" row, whatever name it prints.
    ///
    /// Waits for `applied`, answering the warning on the way, and returns the applied screen.
    async fn await_claude_switch(
        &self,
        step: &str,
        applied: impl Fn(&str) -> bool,
    ) -> Result<String, DomainStateError> {
        let answer = self
            .wait_for(step, |screen| {
                if applied(screen) {
                    return Some(Err(screen.to_string()));
                }
                claude_switch_warning_answer(screen).map(Ok)
            })
            .await?;
        match answer {
            Err(screen) => Ok(screen),
            Ok(key) => {
                self.write(&key).await?;
                self.wait_for(step, |screen| applied(screen).then(|| screen.to_string()))
                    .await
            }
        }
    }

    async fn cancel_dialog(&self) {
        for _ in 0..PICKER_CANCEL_ESCAPES {
            let still_open = self
                .capture()
                .await
                .is_some_and(|screen| any_picker_open(&screen));
            if !still_open {
                return;
            }
            if self.write(SESSION_CHAT_INTERRUPT).await.is_err() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(PICKER_CANCEL_SETTLE_MS)).await;
        }
    }

    async fn run(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        match self.drive(plan).await {
            Ok(()) => Ok(()),
            Err(error) => {
                self.cancel_dialog().await;
                Err(error)
            }
        }
    }

    /// CDXC:AgentScreenDetection 2026-09-05 DECISION:
    /// User: choosing a normal Codex effort uses Shift+Up/Down internally; Max and Ultra must use `/model` and More reasoning instead.
    /// This supersedes sending shifted arrows to every effort level, which only prints Codex's picker-only warning for Max and Ultra.
    /// The fresh footer supplies the baseline inside the serialized send job, so stale client state cannot add the wrong number of steps or split them around another prompt.
    async fn change_effort(
        &self,
        plan: &CodexPickerPlan,
        current: &str,
    ) -> Result<(), DomainStateError> {
        let current_index = CODEX_EFFORTS
            .iter()
            .position(|effort| *effort == current)
            .ok_or_else(|| {
                agent_busy("Codex's current effort could not be read, so it was not changed.")
            })?;
        let target_index = CODEX_EFFORTS
            .iter()
            .position(|effort| *effort == plan.effort)
            .ok_or_else(|| invalid_params(format!("Unknown Codex effort {}.", plan.effort)))?;
        if current_index == target_index {
            return Ok(());
        }
        let key = if target_index > current_index {
            SESSION_CHAT_SHIFT_UP
        } else {
            SESSION_CHAT_SHIFT_DOWN
        };
        let mut index = current_index;
        while index != target_index {
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The effort change was cancelled by another action on this session.",
                ));
            }
            index = if target_index > index {
                index + 1
            } else {
                index - 1
            };
            // Codex can coalesce repeated shifted arrows in one stdin burst.
            // Read each applied step before sending the next one.
            self.write(key).await?;
            self.wait_for("confirm effort", |screen| {
                let selection =
                    detect_session_chat_selection(SessionChatOptionAgent::Codex, screen)?;
                (selection.model.as_ref()?.value == plan.model
                    && selection.effort.as_ref()?.value == CODEX_EFFORTS[index])
                    .then_some(())
            })
            .await?;
        }
        Ok(())
    }

    async fn drive_claude(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        if plan.session_only {
            let result = self.drive_claude_session_only(plan).await;
            if result.is_err() && !(self.cancelled)() {
                self.cancel_dialog().await;
                if let Some(screen) = self.capture().await {
                    if crate::session_chat_composer::claude_composer_input_text(&screen)
                        .is_some_and(|text| {
                            [
                                CLAUDE_MODEL_COMMAND,
                                claude_effort_slider::CLAUDE_EFFORT_COMMAND,
                            ]
                            .contains(&text.trim())
                        })
                    {
                        let _ = self
                            .write(crate::session_chat_send::AGENT_TUI_CLEAR_INPUT_LINE)
                            .await;
                    }
                }
            }
            return result;
        }
        for (field, value) in [
            ("model", plan.model.as_str()),
            ("effort", plan.effort.as_str()),
        ] {
            if value.is_empty() {
                continue;
            }
            if (self.cancelled)() {
                return Err(agent_busy(
                    "The selection was interrupted and remains queued.",
                ));
            }
            let screen = self
                .capture()
                .await
                .ok_or_else(|| session_not_running("The agent's input could not be read."))?;
            let composer = crate::session_chat_composer::detect_session_chat_composer_readiness(
                Some("claude"),
                &screen,
                None,
            );
            if composer.state != crate::session_chat_composer::SessionChatComposerState::Ready {
                return Err(input_box_refusal(
                    &composer,
                    "Waiting for Claude's input box.",
                ));
            }
            // CDXC:SessionChat 2026-09-15 WHY:
            // The effort picker sends the model too. Reapplying an unchanged model added a complete command/confirmation round trip before every effort change, and retries repeated already-applied work.
            let (model, effort) = claude_effort_slider::claude_live_selection(plan, &screen);
            let current = if field == "model" { model } else { effort };
            if current.as_deref() == Some(value) {
                continue;
            }
            let command = format!("/{field} {value}");
            // Never replace a terminal draft while applying a queued setting.
            if crate::session_chat_composer::claude_composer_input_text(&screen)
                .is_some_and(|text| !text.trim().is_empty())
                && self.claude_input_holds_draft().await
            {
                // Option+P opens the model list over the draft; `/effort` has no such shortcut.
                if field == "model" {
                    let mut model_plan = plan.clone();
                    model_plan.effort.clear();
                    self.drive_claude_session_only(&model_plan).await?;
                    continue;
                }
                return Err(agent_busy(
                    "Waiting for the text in Claude's terminal input to be sent or cleared.",
                ));
            }
            // `/effort <level>` cannot turn Ultracode off, so any change to or from it uses the slider.
            if field == "effort"
                && (value == "ultracode" || current.as_deref() == Some("ultracode"))
            {
                self.drive_claude_effort_slider(value, claude_effort_slider::CLAUDE_CONFIRM_KEY)
                    .await?;
                continue;
            }
            let result = self.change_claude_option(field, value, &command).await;
            if result.is_err() && !(self.cancelled)() {
                // Only undo this job's exact input or matching switch confirmation.
                if let Some(screen) = self.capture().await {
                    if let Some(picker) = claude_switch_confirmation(&screen, field, value) {
                        if let Some(key) = picker
                            .rows
                            .iter()
                            .position(|row| row.label == "No, go back")
                            .and_then(|index| picker.answer_key(index))
                        {
                            let _ = self.write(&key).await;
                        }
                    } else if crate::session_chat_composer::claude_composer_input_text(&screen)
                        .is_some_and(|text| text.trim() == command)
                    {
                        let _ = self
                            .write(crate::session_chat_send::AGENT_TUI_CLEAR_INPUT_LINE)
                            .await;
                    }
                }
            }
            result?;
        }
        Ok(())
    }

    /// CDXC:SessionChat 2026-09-05 WHY:
    /// Bracket-pasting `/effort max` reached Claude as an ordinary prompt during live verification.
    /// Option commands use the shared single-line typing encoder, and their cache-change confirmation is part of the serialized job.
    /// Completion requires Claude's acknowledgement below the exact command after the dialog closes: the footer calls both Opus variants "Opus", so it cannot prove which variant was applied.
    async fn change_claude_option(
        &self,
        field: &str,
        value: &str,
        command: &str,
    ) -> Result<(), DomainStateError> {
        self.write(&crate::session_chat_send::build_session_chat_paste_bytes(
            command,
        ))
        .await?;
        self.wait_for("type Claude option command", |screen| {
            crate::session_chat_composer::claude_composer_input_text(screen)
                .is_some_and(|text| text.trim() == command)
                .then_some(())
        })
        .await?;
        self.write(CODEX_SUBMIT).await?;
        let answer = self
            .wait_for("Claude option confirmation", |screen| {
                if let Some(picker) = claude_switch_confirmation(screen, field, value) {
                    return picker
                        .rows
                        .iter()
                        .position(|row| row.label.starts_with("Yes, switch to "))
                        .and_then(|index| picker.answer_key(index))
                        .map(Some);
                }
                claude_option_applied(screen, field, value).then_some(None)
            })
            .await?;
        if let Some(answer) = answer {
            self.write(&answer).await?;
            self.wait_for("applied Claude option", |screen| {
                claude_option_applied(screen, field, value).then_some(())
            })
            .await?;
        }
        Ok(())
    }

    /// CDXC:SessionChat 2026-09-27 DECISION:
    /// User: Codex uses its own "this session only" choice, which Codex 0.157 added to its reasoning lists as `s session`. A session pick highlights the reasoning row with arrows and answers `s`, and succeeds only on Codex's "Model changed to <model> <effort> for this conversation".
    /// A row digit is never used for it (a digit saves the default), nor the Shift+Up/Down reasoning shortcut, whose effect on `~/.codex/config.toml` is unverified.
    /// SEE-ALSO: server/src/session_chat_model_selection.rs read_scope, packages/gx-chat-core/src/menus/picker/model_picker.rs model_picker_supports_session_scope.
    ///
    /// Each press is read back before the next, because Codex can coalesce a burst of keys.
    async fn answer_codex_row_for_session(
        &self,
        step: &str,
        to: u32,
        rows: impl Fn(&str) -> Option<Vec<PickerRow>>,
    ) -> Result<(), DomainStateError> {
        let highlighted = |screen: &str| {
            rows(screen)?
                .into_iter()
                .find(|row| row.selected)
                .map(|row| row.number)
        };
        for _ in 0..CODEX_ROW_STEP_LIMIT {
            let from = self.wait_for(step, highlighted).await?;
            if from == to {
                return self.write(CODEX_SESSION_KEY).await;
            }
            self.write(if to > from {
                CODEX_ARROW_DOWN
            } else {
                CODEX_ARROW_UP
            })
            .await?;
            self.wait_for(step, |screen| {
                highlighted(screen).filter(|landed| *landed != from)
            })
            .await?;
        }
        Err(dialog_mismatch(
            step,
            "the highlight never reached the row, so it was not applied.",
        ))
    }

    async fn drive(&self, plan: &CodexPickerPlan) -> Result<(), DomainStateError> {
        // Re-checked here, one instant before the first keystroke, because the
        // handler's check ran before this job reached the front of the queue.
        let screen = self.capture().await.ok_or_else(|| {
            session_not_running(
                "The session's screen could not be read, so the model was not changed.",
            )
        })?;
        let composer = crate::session_chat_composer::detect_session_chat_composer_readiness(
            Some("codex"),
            &screen,
            None,
        );
        if composer.state != crate::session_chat_composer::SessionChatComposerState::Ready {
            return Err(input_box_refusal(
                &composer,
                "Codex is not showing its input box, so the model was not changed.",
            ));
        }

        let selection = detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen)
            .ok_or_else(|| {
                agent_busy(
                    "Codex's current model could not be read, so its settings were not changed.",
                )
            })?;
        if selection
            .model
            .as_ref()
            .is_some_and(|model| model.value == plan.model)
        {
            let effort = selection.effort.as_ref().ok_or_else(|| {
                agent_busy("Codex's current effort could not be read, so it was not changed.")
            })?;
            if effort.value == plan.effort {
                return Ok(());
            }
            // A session pick always goes through the list's `s`: the reasoning shortcut is not
            // known to leave the saved default alone.
            if !plan.session_only && !matches!(plan.effort.as_str(), "max" | "ultra") {
                return self.change_effort(plan, &effort.value).await;
            }
        }

        self.write(CODEX_MODEL_COMMAND).await?;
        self.wait_for("type command", |screen| {
            screen_lines(screen)
                .iter()
                .any(|line| {
                    line.strip_prefix(CODEX_CURSOR)
                        .or_else(|| line.strip_prefix(CODEX_ULTRA_CURSOR))
                        .is_some_and(|text| text.trim() == CODEX_MODEL_COMMAND)
                })
                .then_some(())
        })
        .await?;
        self.write(CODEX_SUBMIT).await?;

        let model_rows = self.wait_for("open", model_picker_rows).await?;
        let Some(model_row) = model_rows.iter().find(|row| row_names(row, &plan.model)) else {
            let listed = model_rows
                .iter()
                .map(|row| row.text.split(' ').next().unwrap_or_default().to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(unsupported_selection(format!(
                "Codex's model picker lists {listed}, not {}.",
                plan.model
            )));
        };
        self.write(&row_key(model_row, "model")?).await?;

        let effort_label = effort_row_label(&plan.effort)
            .ok_or_else(|| invalid_params(format!("Unknown Codex effort {}.", plan.effort)))?;
        let effort_rows = self
            .wait_for("effort", |screen| effort_picker_rows(screen, &plan.model))
            .await?;
        if plan.session_only
            && !self
                .capture()
                .await
                .is_some_and(|screen| effort_picker_offers_session(&screen, &plan.model))
        {
            return Err(unsupported_selection(
                "This Codex version cannot apply a model to one conversation. Update Codex, or save the pick as the default.",
            ));
        }
        if let Some(row) = effort_rows.iter().find(|row| row_names(row, effort_label)) {
            if plan.session_only {
                self.answer_codex_row_for_session("effort", row.number, |screen| {
                    effort_picker_rows(screen, &plan.model)
                })
                .await?;
            } else {
                self.write(&row_key(row, "effort")?).await?;
            }
        } else if let Some(more) = effort_rows
            .iter()
            .find(|row| row.text.starts_with(CODEX_MORE_REASONING_ROW))
        {
            // Max and Ultra sit behind "More reasoning…"; open it and pick there.
            self.write(&row_key(more, "effort")?).await?;
            let row = self
                .wait_for("more reasoning", |screen| {
                    advanced_effort_picker_rows(screen)
                        .and_then(|rows| rows.into_iter().find(|row| row_names(row, effort_label)))
                })
                .await?;
            if plan.session_only {
                self.answer_codex_row_for_session(
                    "more reasoning",
                    row.number,
                    advanced_effort_picker_rows,
                )
                .await?;
            } else {
                self.write(&row_key(&row, "more reasoning")?).await?;
            }
        } else {
            let listed = effort_rows
                .iter()
                .map(|row| row.text.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(unsupported_selection(format!(
                "Codex's {} offers {listed}, not {effort_label}.",
                plan.model
            )));
        }

        self.wait_for("confirm", |screen| {
            let selection = detect_session_chat_selection(SessionChatOptionAgent::Codex, screen)?;
            // Earlier visits can leave the same confirmation line in scrollback.
            (changed_line_present(screen, &plan.model, &plan.effort, plan.session_only)
                && !any_picker_open(screen)
                && selection.model.as_ref()?.value == plan.model
                && selection.effort.as_ref()?.value == plan.effort)
                .then_some(())
        })
        .await
    }
}

pub(crate) async fn run_codex_model_picker_job(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    job_id: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) {
    let plan = picker_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_id).map(|job| job.plan.clone()));
    let Some(plan) = plan else {
        return;
    };
    let driver = PickerDriver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = async {
        if !plan.model.is_empty() {
            driver.run(&plan).await?;
        }
        driver
            .drive_options(&plan, SessionChatOptionAgent::Codex)
            .await
    }
    .await;
    if let Err(error) = outcome.as_ref() {
        log_picker(
            LogLevel::Error,
            "sessionChatCodexModelPickFailed",
            json!({
                "projectId": project_id,
                "providerSessionId": zmx_name,
                "sessionId": session_id,
                "code": error.code,
                "model": plan.model,
                "effort": plan.effort,
            }),
            Some(error.message.clone()),
        );
    }
    if let Ok(mut jobs) = picker_jobs().lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            job.outcome = Some(outcome);
        }
    }
}

pub(crate) async fn run_claude_model_picker_job(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &str,
    job_id: u64,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) {
    let plan = picker_jobs()
        .lock()
        .ok()
        .and_then(|jobs| jobs.get(&job_id).map(|job| job.plan.clone()));
    let Some(plan) = plan else {
        return;
    };
    let driver = PickerDriver {
        project_id,
        session_id,
        zmx_name,
        source,
        cancelled,
    };
    let outcome = async {
        if !plan.model.is_empty() {
            driver.drive_claude(&plan).await?;
        }
        driver
            .drive_options(&plan, SessionChatOptionAgent::Claude)
            .await
    }
    .await;
    if let Err(error) = outcome.as_ref() {
        log_picker(
            LogLevel::Error,
            "sessionChatClaudeModelPickFailed",
            json!({
                "projectId": project_id,
                "providerSessionId": zmx_name,
                "sessionId": session_id,
                "code": error.code,
                "model": plan.model,
                "effort": plan.effort,
            }),
            Some(error.message.clone()),
        );
    }
    if let Ok(mut jobs) = picker_jobs().lock() {
        if let Some(job) = jobs.get_mut(&job_id) {
            job.outcome = Some(outcome);
        }
    }
}

// ---------------------------------------------------------------------------
// /api/selectSessionChatModel
// ---------------------------------------------------------------------------

pub(crate) async fn handle_select_session_chat_model_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => return domain_error_response(endpoint_path, request_id, error),
    };
    let result = if params.get("defer").and_then(Value::as_bool) == Some(true) {
        crate::session_chat_model_selection::enqueue(state, &params)
    } else {
        select_session_chat_model(state, &params).await
    };
    if let Err(error) = &result {
        // A pick the queue is not carrying (`ghostex select-session-chat-model`): the queued ones
        // remember theirs in session_chat_model_selection.rs.
        let (project_id, session_id) = (
            read_trimmed(&params, "projectId"),
            read_trimmed(&params, "sessionId"),
        );
        crate::session_chat_mode_availability::remember_refusal(
            state,
            &project_id,
            &session_id,
            &error.message,
        );
        crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
            state,
            &project_id,
            &session_id,
        );
    }
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn read_trimmed(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

/// Rewrites the session's saved agent command so a resume relaunches on the model that is
/// running now. Only `claude` and `codex` carry model and effort as launch options; every other
/// provider keeps its command untouched.
fn persist_session_chat_model_resume(
    state: &AppState,
    target: &crate::session_chat_send::SessionChatSendTarget,
    model: &str,
    effort: &str,
    session_only: bool,
) -> Result<(), DomainStateError> {
    let agent = crate::session_chat_follower::session_chat_agent_for_session(&target.session)
        .unwrap_or_default();
    if !matches!(agent.as_str(), "claude" | "codex" | "cursor") || model.is_empty() {
        return Ok(());
    }
    // CDXC:AgentProviders 2026-10-07: a pick saved as the default is what new sessions start on;
    // a pick for this session alone is never remembered as the default.
    let pin = crate::agent_model_pins::ModelPin {
        model: model.to_string(),
        effort: (!effort.is_empty()).then(|| effort.to_string()),
        ..Default::default()
    };
    if !session_only {
        crate::agent_model_pins::record_default(&agent, pin.clone());
    }
    crate::agent_model_pins::record_session_choice(
        &target.project_id,
        &target.session_id,
        &agent,
        pin.clone(),
    );
    let db =
        crate::storage::open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })?;
    let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
    let session = repository
        .get_session(&target.project_id, &target.session_id)?
        .ok_or_else(|| DomainStateError {
            code: "notFound",
            message: "The session no longer exists.".into(),
        })?;
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let marker = crate::agent_model_pins::session_marker(
        if session_only {
            crate::agent_model_pins::ORIGIN_SESSION
        } else {
            crate::agent_model_pins::ORIGIN_DEFAULT
        },
        Some(&pin),
    );
    let marker_changed = runtime.get(crate::agent_model_pins::SESSION_PIN_KEY) != Some(&marker);
    runtime.insert(crate::agent_model_pins::SESSION_PIN_KEY.to_string(), marker);
    let command = runtime
        .get("agentCommand")
        .and_then(Value::as_str)
        .filter(|command| !command.trim().is_empty())
        .map(str::to_string);
    let rewritten = command
        .as_deref()
        .map(|command| {
            crate::agents::with_agent_model_options(
                command,
                &agent,
                Some(model),
                (!effort.is_empty() && agent != "cursor").then_some(effort),
            )
        })
        .transpose()?;
    if rewritten == command && !marker_changed {
        return Ok(());
    }
    if let Some(rewritten) = rewritten {
        runtime.insert("agentCommand".into(), json!(rewritten));
    }
    repository.update_session(
        json!({
            "projectId": target.project_id,
            "sessionId": target.session_id,
            "runtimeSettings": runtime,
        })
        .as_object()
        .unwrap(),
    )?;
    Ok(())
}

pub(crate) async fn select_session_chat_model(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let model = read_trimmed(params, "model");
    let effort = read_trimmed(params, "effort");
    if model.is_empty() && !effort.is_empty() {
        return Err(invalid_params("An effort change requires its model."));
    }
    let target = resolve_session_chat_send_target(state, params, "selectSessionChatModel")?;
    let agent = crate::session_chat_follower::session_chat_agent_for_session(&target.session);
    if agent.as_deref() == Some("opencode") {
        // A chat's model can be picked before its first message, so the conversation may not exist yet.
        let conversation = crate::session_chat_opencode::chat_conversation(
            state,
            &target.project_id,
            &target.session_id,
            &target.session,
        )?;
        let args = params.clone();
        let result = tokio::task::spawn_blocking(move || {
            crate::session_chat_opencode::ensure_conversation(
                &conversation.id,
                conversation.directory.as_deref(),
            )?;
            crate::session_chat_opencode::select(&conversation.id, &args)
        })
        .await
        .map_err(|_| invalid_params("OpenCode model operation failed."))??;
        crate::session_chat_options::schedule_session_chat_option_redetect(
            state,
            &target.project_id,
            &target.session_id,
            Some("opencode"),
        );
        return Ok(result);
    }
    // OMP shares Pi's transcript family; its commands and footer are its own.
    let agent = match agent.as_deref() {
        Some("pi") => crate::session_chat_composer::session_chat_composer_agent_id(&target.session)
            .filter(|display| display == "omp")
            .or(agent),
        _ => agent,
    };
    if !matches!(
        agent.as_deref(),
        Some(
            "codex"
                | "claude"
                | "cursor"
                | "grok"
                | "antigravity"
                | "hermes"
                | "pi"
                | "omp"
                | "empryo"
                | "dsh"
        )
    ) {
        return Err(DomainStateError {
            code: "unsupportedAgent",
            message: "This agent does not support the quick model picker.".to_string(),
        });
    }
    let options = crate::session_chat_model_selection::read_options(
        agent.as_deref().unwrap_or_default(),
        params,
    )?;
    let scope = crate::session_chat_model_selection::read_scope(
        agent.as_deref().unwrap_or_default(),
        params,
    )?;
    let session_only = scope == crate::session_chat_model_selection::SCOPE_SESSION;
    if session_only && model.is_empty() {
        return Err(invalid_params(
            "A session-only change needs the model to apply in this session.",
        ));
    }
    if !model.is_empty() || options.is_empty() {
        crate::session_chat_model_selection::validate_selection(
            agent.as_deref().unwrap_or_default(),
            &model,
            &effort,
        )?;
    }
    if let Some(refusal) = crate::session_chat_model_selection::foreign_model_refusal(
        agent.as_deref().unwrap_or_default(),
        &model,
    ) {
        return Err(unsupported_selection(refusal));
    }
    if !model.is_empty() && agent.as_deref() == Some("codex") && effort_row_label(&effort).is_none()
    {
        return Err(invalid_params(format!("Unknown Codex effort {effort}.")));
    }
    if crate::presentation::effective_lifecycle_state(&target.session) != "running" {
        return Err(session_not_running(
            "Waiting for the session to be running before applying the selection.",
        ));
    }
    let Some(_guard) = PickInFlightGuard::claim(&target.project_id, &target.session_id) else {
        return Err(agent_busy(
            "A model change is already running for this session.",
        ));
    };
    // Reads the profile's session store and config, so it stays off the async workers.
    let hermes_provider = if agent.as_deref() == Some("hermes") {
        let (session, model) = (target.session.clone(), model.clone());
        tokio::task::spawn_blocking(move || {
            crate::session_chat_hermes_status::hermes_switch_provider(&session, &model)
        })
        .await
        .ok()
        .flatten()
    } else {
        None
    };
    let job_id = register_job(CodexPickerPlan {
        provider: agent.clone().unwrap_or_default(),
        model: model.clone(),
        effort: effort.clone(),
        options,
        session_only,
        claude_statusline: crate::server::read_runtime_text(&target.session, "agentSessionId")
            .map(|id| (state.paths.app_state_dir.join("agent-hooks"), id)),
        hermes_provider,
        empryo_session_log: empryo_picker::plan_session_log(
            state,
            agent.as_deref(),
            &target.session,
        ),
    });
    let send = execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-model-picker",
        vec![if agent.as_deref() == Some("claude") {
            SessionChatSendStep::DriveClaudeModelPicker { job_id }
        } else if agent.as_deref() == Some("codex") {
            SessionChatSendStep::DriveCodexModelPicker { job_id }
        } else {
            SessionChatSendStep::DriveProviderModelPicker { job_id }
        }],
    )
    .await;
    let outcome = take_job_outcome(job_id);
    match (send, outcome) {
        (_, Some(Err(error))) => return Err(error),
        (Err(error), _) => {
            return Err(agent_busy(format!(
                "{} The model was not changed.",
                error.message
            )));
        }
        (Ok(()), None) => {
            return Err(agent_busy(
                "The session's terminal queue dropped the model change before it ran.",
            ));
        }
        (Ok(()), Some(Ok(()))) => {}
    }
    // Publish the completed selection promptly and recheck slower footer repaints.
    crate::session_chat_options::schedule_session_chat_option_redetect(
        state,
        &target.project_id,
        &target.session_id,
        agent.as_deref(),
    );
    // CDXC:SessionChat 2026-09-18 DECISION:
    // User: every pick rewrites the session's saved resume command, so waking a slept pane comes
    // back on the model that is running rather than the one it was launched with. Without this a
    // session-only pick would silently mean "until this pane sleeps", and a default-changing pick
    // on a session launched with --model/--effort would snap back to its launch flags.
    // SEE-ALSO: server/src/agents/session_command.rs with_agent_model_options, server/src/agents/launch_plan.rs.
    if let Err(error) =
        persist_session_chat_model_resume(state, &target, &model, &effort, session_only)
    {
        log_picker(
            LogLevel::Warn,
            "sessionChatModelResumeCommandUnchanged",
            json!({
                "projectId": target.project_id,
                "sessionId": target.session_id,
                "model": model,
                "effort": effort,
            }),
            Some(error.message),
        );
    }
    log_picker(
        LogLevel::Info,
        "sessionChatCodexModelPicked",
        json!({
            "projectId": target.project_id,
            "sessionId": target.session_id,
            "model": model,
            "effort": effort,
            "scope": scope,
        }),
        None,
    );
    Ok(json!({ "ok": true, "model": model, "effort": effort, "scope": scope }))
}

#[cfg(test)]
#[path = "session_chat_codex_picker_tests.rs"]
mod tests;
