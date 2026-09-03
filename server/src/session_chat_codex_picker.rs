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
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, resolve_session_chat_send_target,
    write_session_chat_payload, SessionChatSendStep, SESSION_CHAT_INTERRUPT,
};

/// Typed as literal keystrokes so the composer's slash popup runs it.
const CODEX_MODEL_COMMAND: &str = "/model";
const CODEX_SUBMIT: &str = "\r";

const CODEX_MODEL_PICKER_TITLE: &str = "Select Model and Effort";
const CODEX_EFFORT_PICKER_TITLE_PREFIX: &str = "Select Reasoning Level for ";
/// Row that opens the Max / Ultra sub-list.
const CODEX_MORE_REASONING_ROW: &str = "More reasoning";
/// The line Codex prints once the picker has applied the change.
const CODEX_MODEL_CHANGED_PREFIX: &str = "Model changed to ";
/// Highlight marker on the picker's current row (and Codex's composer prompt).
const CODEX_CURSOR: char = '\u{203a}';

const PICKER_POLL_MS: u64 = 150;
const PICKER_STEP_TIMEOUT_MS: u64 = 6_000;
/// Settle between typing the command and submitting it, so the two writes
/// reach the TUI in separate stdin chunks.
const PICKER_COMMAND_SETTLE_MS: u64 = 300;
/// Escapes written to close whatever the drive left open: the effort list
/// backs out to the model list, and the model list to the composer.
const PICKER_CANCEL_ESCAPES: usize = 2;

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
        message: format!("Codex did not answer the model picker's {step} step in time."),
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

/// The last run of consecutive numbered rows on screen, for the Max / Ultra
/// sub-list, whose heading Ghostex does not pin. Any non-row line ends a run.
fn last_numbered_run(screen: &str) -> Option<Vec<PickerRow>> {
    let mut runs: Vec<Vec<PickerRow>> = Vec::new();
    let mut current: Vec<PickerRow> = Vec::new();
    for line in screen_lines(screen) {
        match parse_picker_row(&line) {
            Some(row) => current.push(row),
            None => {
                if !current.is_empty() {
                    runs.push(std::mem::take(&mut current));
                }
            }
        }
    }
    if !current.is_empty() {
        runs.push(current);
    }
    runs.pop()
}

fn any_picker_open(screen: &str) -> bool {
    screen_lines(screen).iter().any(|line| {
        line == CODEX_MODEL_PICKER_TITLE || line.starts_with(CODEX_EFFORT_PICKER_TITLE_PREFIX)
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
            tokio::time::sleep(Duration::from_millis(PICKER_COMMAND_SETTLE_MS)).await;
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

        self.write(CODEX_MODEL_COMMAND).await?;
        tokio::time::sleep(Duration::from_millis(PICKER_COMMAND_SETTLE_MS)).await;
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
                    last_numbered_run(screen)
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
            changed_line_present(screen, &plan.model, &plan.effort).then_some(())
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
    match select_session_chat_model(state, &params).await {
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

async fn select_session_chat_model(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let model = read_trimmed(params, "model");
    let effort = read_trimmed(params, "effort");
    if model.is_empty() || effort.is_empty() {
        return Err(invalid_params(
            "selectSessionChatModel requires model and effort.",
        ));
    }
    if effort_row_label(&effort).is_none() {
        return Err(invalid_params(format!("Unknown Codex effort {effort}.")));
    }
    let target = resolve_session_chat_send_target(state, params, "selectSessionChatModel")?;
    let agent = crate::session_chat_follower::session_chat_agent_for_session(&target.session);
    if agent.as_deref() != Some("codex") {
        return Err(DomainStateError {
            code: "unsupportedAgent",
            message: "Choosing the model from chat is only available for Codex sessions."
                .to_string(),
        });
    }
    if crate::presentation::effective_lifecycle_state(&target.session) != "running" {
        return Err(session_not_running(
            "The session is not running, so Codex has no model picker to drive.",
        ));
    }
    let generated_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    if crate::presentation::presentation_activity(&target.session, &generated_at) == "working" {
        return Err(agent_busy(
            "Codex is still working on a turn. Wait for it to finish, or stop it, and then change the model.",
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
        vec![SessionChatSendStep::DriveCodexModelPicker { job_id }],
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
    // The footer repaints within a second; the post-dispatch redetect reads it
    // at +2s and +6s and republishes the pills.
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
