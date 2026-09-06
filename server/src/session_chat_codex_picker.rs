/*
CDXC:AgentScreenDetection 2026-09-03 DECISION:
User: "i want codex to list the available models and efforts and if fast is enabled/disabled (like claude does now)", and the model must be changeable from the chat box, not only from the terminal.

CDXC:AgentScreenDetection 2026-09-03 WHY:
Codex has no non-interactive way to set the model. `/model <name>` is not a command: the CLI sends it to the model as a prompt and starts a turn (measured on Codex 0.153). The only channel is its own `/model` picker: a numbered "Select Model and Effort" list, then a numbered "Select Reasoning Level for <model>" list, where a digit both selects and confirms a row. So `/api/selectSessionChatModel` drives that picker in the session's terminal the way the Claude rewind driver drives `/rewind`: every keystroke is preceded by a screen capture that must show the expected list, the digit is read off the row that names the requested model (never computed from a catalog index), and any capture that disagrees aborts with Escape instead of typing into whatever is on screen. The whole drive is one job on the session's serialized send worker, so a queued prompt can never land inside the picker.
SEE-ALSO: packages/core-ui/chat/session-chat-session-options.ts (the `model-picker` dispatch), session_chat_rewind.rs (the driver pattern).
*/

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

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
const CODEX_ULTRA_CURSOR: char = '\u{00bb}';

const PICKER_POLL_MS: u64 = 150;
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
}

/// `› 2. gpt-5.6-terra (current) Balanced agentic …` → number 2, text after the dot.
fn parse_picker_row(line: &str) -> Option<PickerRow> {
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

fn effort_picker_rows(screen: &str, model: &str) -> Option<Vec<PickerRow>> {
    let expected = format!("{CODEX_EFFORT_PICKER_TITLE_PREFIX}{model}");
    picker_rows_under(&screen_lines(screen), |line| line == expected)
}

fn advanced_effort_picker_rows(screen: &str) -> Option<Vec<PickerRow>> {
    picker_rows_under(&screen_lines(screen), |line| {
        line == CODEX_ADVANCED_REASONING_TITLE
    })
}

fn any_picker_open(screen: &str) -> bool {
    screen_lines(screen).iter().any(|line| {
        line == CODEX_MODEL_PICKER_TITLE
            || line.starts_with(CODEX_EFFORT_PICKER_TITLE_PREFIX)
            || line == CODEX_ADVANCED_REASONING_TITLE
    })
}

/// Whether a row's text is the model or effort label `wanted`: the label is
/// the whole text, or the text continues with a space (` (current)`, ` (default)`,
/// or the row's description).
fn row_names(row: &PickerRow, wanted: &str) -> bool {
    row.text == wanted
        || row
            .text
            .strip_prefix(wanted)
            .is_some_and(|rest| rest.starts_with(' '))
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

fn changed_line_present(screen: &str, model: &str, effort: &str) -> bool {
    let expected = format!("{CODEX_MODEL_CHANGED_PREFIX}{model} {effort}");
    screen_lines(screen).iter().any(|line| {
        let rest = line
            .strip_prefix(CODEX_CURSOR)
            .map(str::trim_start)
            .unwrap_or(line);
        rest.trim_start_matches(['•', ' ']).starts_with(&expected)
    })
}

// ---------------------------------------------------------------------------
// Job registry the send worker's step reads
// ---------------------------------------------------------------------------

#[derive(Clone, Debug)]
struct CodexPickerPlan {
    model: String,
    effort: String,
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
        .replace(' ', "-");
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

    /// Polls the screen until `accept` answers `Some`, the step deadline
    /// passes, or the session's send generation is superseded.
    async fn wait_for<T>(
        &self,
        step: &str,
        mut accept: impl FnMut(&str) -> Option<T>,
    ) -> Result<T, DomainStateError> {
        let deadline = std::time::Instant::now() + Duration::from_millis(PICKER_STEP_TIMEOUT_MS);
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
            tokio::time::sleep(Duration::from_millis(PICKER_POLL_MS)).await;
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
                return Err(agent_busy("Waiting for Claude's input box."));
            }
            let command = format!("/{field} {value}");
            // Never replace a terminal draft while applying a queued setting.
            if crate::session_chat_composer::claude_composer_input_text(&screen)
                .is_some_and(|text| !text.trim().is_empty())
            {
                return Err(agent_busy(
                    "Waiting for the text in Claude's terminal input to be sent or cleared.",
                ));
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
            return Err(agent_busy(
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
            if !matches!(plan.effort.as_str(), "max" | "ultra") {
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
            return Err(dialog_mismatch(
                "model",
                &format!("the picker lists {listed}, not {}.", plan.model),
            ));
        };
        self.write(&row_key(model_row, "model")?).await?;

        let effort_label = effort_row_label(&plan.effort)
            .ok_or_else(|| invalid_params(format!("Unknown Codex effort {}.", plan.effort)))?;
        let effort_rows = self
            .wait_for("effort", |screen| effort_picker_rows(screen, &plan.model))
            .await?;
        if let Some(row) = effort_rows.iter().find(|row| row_names(row, effort_label)) {
            self.write(&row_key(row, "effort")?).await?;
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
            self.write(&row_key(&row, "more reasoning")?).await?;
        } else {
            let listed = effort_rows
                .iter()
                .map(|row| row.text.clone())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(dialog_mismatch(
                "effort",
                &format!("{} offers {listed}, not {effort_label}.", plan.model),
            ));
        }

        self.wait_for("confirm", |screen| {
            let selection = detect_session_chat_selection(SessionChatOptionAgent::Codex, screen)?;
            // Earlier visits can leave the same confirmation line in scrollback.
            (changed_line_present(screen, &plan.model, &plan.effort)
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
    let outcome = driver.run(&plan).await;
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
    let outcome = driver.drive_claude(&plan).await;
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

pub(crate) async fn select_session_chat_model(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let model = read_trimmed(params, "model");
    let effort = read_trimmed(params, "effort");
    if model.is_empty() {
        return Err(invalid_params(
            "selectSessionChatModel requires model and effort.",
        ));
    }
    let target = resolve_session_chat_send_target(state, params, "selectSessionChatModel")?;
    let agent = crate::session_chat_follower::session_chat_agent_for_session(&target.session);
    if !matches!(agent.as_deref(), Some("codex" | "claude")) {
        return Err(DomainStateError {
            code: "unsupportedAgent",
            message: "Choosing the model from chat is available for Codex and Claude sessions."
                .to_string(),
        });
    }
    crate::session_chat_model_selection::validate_selection(
        agent.as_deref().unwrap_or_default(),
        &model,
        &effort,
    )?;
    if agent.as_deref() == Some("codex") && effort_row_label(&effort).is_none() {
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
    let job_id = register_job(CodexPickerPlan {
        model: model.clone(),
        effort: effort.clone(),
    });
    let send = execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-model-picker",
        vec![if agent.as_deref() == Some("claude") {
            SessionChatSendStep::DriveClaudeModelPicker { job_id }
        } else {
            SessionChatSendStep::DriveCodexModelPicker { job_id }
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
            )))
        }
        (Ok(()), None) => {
            return Err(agent_busy(
                "The session's terminal queue dropped the model change before it ran.",
            ))
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
    log_picker(
        LogLevel::Info,
        "sessionChatCodexModelPicked",
        json!({
            "projectId": target.project_id,
            "sessionId": target.session_id,
            "model": model,
            "effort": effort,
        }),
        None,
    );
    Ok(json!({ "ok": true, "model": model, "effort": effort }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ultra_composer_is_ready_but_advanced_picker_is_not() {
        use crate::session_chat_composer::{
            detect_session_chat_composer_ready, SessionChatComposerState,
        };
        assert_eq!(
            detect_session_chat_composer_ready(
                Some("codex"),
                "» Ask Codex to do anything\n  gpt-6-astra ultra"
            )
            .state,
            SessionChatComposerState::Ready
        );
        let advanced = "Advanced Reasoning\n⚠ Consumes usage limits faster\n› 1. Max  Higher usage\n  2. Ultra  Highest usage\nPress enter to confirm or esc to go back";
        assert!(any_picker_open(advanced));
        assert_eq!(advanced_effort_picker_rows(advanced).unwrap().len(), 2);
        assert_eq!(
            detect_session_chat_composer_ready(Some("codex"), advanced).state,
            SessionChatComposerState::NotReady
        );
    }

    /// Run this test binary from the packaged Web directory so bundled zmx resolves.
    /// Set `GHOSTEX_CODEX_PICKER_TEST_ZMX=ghostex-effort-check-...` to a dedicated idle Codex TUI and pass `live_zmx_effort_round_trip --ignored --nocapture`.
    #[tokio::test]
    #[ignore = "requires an explicitly supplied disposable Codex zmx session"]
    async fn live_zmx_effort_round_trip() {
        let name = std::env::var("GHOSTEX_CODEX_PICKER_TEST_ZMX")
            .expect("set the dedicated test session name");
        assert!(name.starts_with("ghostex-effort-check-"));
        let screen = capture_session_terminal_text(&name).await.unwrap();
        let initial =
            detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
        let model = initial.model.unwrap().value;
        let original_effort = initial.effort.unwrap().value;
        let warning = "Max and Ultra are available under";
        let warning_count = screen.matches(warning).count();
        let mut failure = None;
        for effort in [
            "high",
            "xhigh",
            "max",
            "ultra",
            "max",
            "low",
            "ultra",
            "high",
            original_effort.as_str(),
        ] {
            let started = std::time::Instant::now();
            let job_id = register_job(CodexPickerPlan {
                model: model.clone(),
                effort: effort.to_string(),
            });
            let send = execute_session_chat_send(
                "effort-check",
                &name,
                &name,
                "effort-check",
                vec![SessionChatSendStep::DriveCodexModelPicker { job_id }],
            )
            .await;
            let result = take_job_outcome(job_id).expect("picker worker outcome");
            if let Err(error) = result {
                failure = Some(error.message);
                break;
            }
            assert!(send.is_ok());
            let screen = capture_session_terminal_text(&name).await.unwrap();
            let selection =
                detect_session_chat_selection(SessionChatOptionAgent::Codex, &screen).unwrap();
            assert_eq!(selection.model.unwrap().value, model);
            assert_eq!(selection.effort.unwrap().value, effort);
            assert_eq!(
                screen.matches(warning).count(),
                warning_count,
                "shifted arrows must not attempt Max/Ultra"
            );
            assert!(!any_picker_open(&screen));
            println!(
                "{model} {effort}: {}ms, footer confirmed and picker closed",
                started.elapsed().as_millis()
            );
        }
        if let Some(error) = failure {
            panic!(
                "{error}\n{}",
                capture_session_terminal_text(&name)
                    .await
                    .unwrap_or_default()
            );
        }
    }
}
