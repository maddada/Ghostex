use std::{
    collections::HashMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
    time::Duration,
};

use tokio::sync::{mpsc, oneshot};

use crate::session_chat::{SessionChatQuestion, SessionChatQuestionSelection};

/*
CDXC:SessionChatSend 2026-07-31:
Session Chat send path (upstream chat spec §7/§8 port). The agent is a TUI, so sending is
writing bytes to its pty via `zmx send` stdin. The spec's measured discipline is
preserved verbatim: a Ctrl+U/Ctrl+K clear burst sized by the 2N-1 law, a
bracketed-paste body with ESC sanitized and newlines normalized to CR, and a
SEPARATE Enter write after a 500ms settle (a trailing \r inside the paste
burst is read as newline text and the message stays staged). A per-session
queue serializes sequences: each send owns the input line from its clear
until its Enter fires. HTTP handlers enqueue and return immediately.
*/

// Master constant table (upstream chat spec §7.1).
pub const SESSION_CHAT_SUBMIT_DELAY_MS: u64 = 500;
pub const SESSION_CHAT_QUESTION_STEP_MS: u64 = 1_000;
pub const SESSION_CHAT_IMAGE_ATTACHMENT_SETTLE_MS: u64 = 300;
pub const SESSION_CHAT_SUBMIT: &str = "\r";
/*
Esc in the kitty CSI-u encoding (CSI 27 u). Ghostex agent sessions always run
under zmx, whose VT layer answers the kitty keyboard-protocol query, so Claude
Code runs with the protocol enabled and a lone 0x1b byte is never delivered as
an Esc keypress (it reads as the ambiguous start of a sequence and is dropped).
Verified live 2026-08-01 against Claude Code v2.1.220 on a zmx pty: "\x1b" did
not interrupt a running turn; "\x1b[27u" interrupted immediately. Crossterm-
based TUIs (codex) parse CSI-u Esc as well, so one encoding covers both.
*/
pub const SESSION_CHAT_INTERRUPT: &str = "\u{1b}[27u";
/*
Shift+Tab in the kitty CSI-u encoding (CSI 9 ; 2 u — Tab with the Shift
modifier). Claude Code cycles its permission mode on it and has no
slash-command equivalent, so the chat surface injects the raw bytes.
Verified live 2026-08-01 against Claude Code v2.1.220 on a zmx pty: the legacy
back-tab "\x1b[Z" did nothing, while "\x1b[9;2u" cycled the footer through
auto → manual → accept edits → plan → bypass on every write. Same kitty-active
reasoning as SESSION_CHAT_INTERRUPT above.
*/
pub const SESSION_CHAT_SHIFT_TAB: &str = "\u{1b}[9;2u";
pub const SESSION_CHAT_SHIFT_UP: &str = "\u{1b}[1;2A";
pub const SESSION_CHAT_SHIFT_DOWN: &str = "\u{1b}[1;2B";
pub const AGENT_TUI_CLEAR_INPUT_LINE: &str = "\u{15}"; // Ctrl+U — clear toward start
pub const AGENT_TUI_CLEAR_INPUT_FORWARD: &str = "\u{b}"; // Ctrl+K — clear toward end
pub const AGENT_TUI_CLEAR_LINE_SLACK: usize = 8;
pub const AGENT_TUI_CLEAR_MAX_LINES: usize = 40;
const SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT: Duration = Duration::from_secs(16);
const PROMPT_STASH_REQUEST_FRESHNESS: Duration = Duration::from_secs(15);
const BRACKETED_PASTE_START: &str = "\u{1b}[200~";
const BRACKETED_PASTE_END: &str = "\u{1b}[201~";
static SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE: AtomicU64 = AtomicU64::new(1);

// Ask-answer keystrokes (upstream chat spec §8.4/§8.5).
const ASK_ENTER: &str = "\r";
const ASK_NEXT_TAB: &str = "\u{1b}[C"; // Right arrow → next question / Submit tab
const ASK_PREVIOUS_ROW: &str = "\u{1b}[A"; // Up
const ASK_NEXT_ROW: &str = "\u{1b}[B"; // Down
const ASK_NOTES: &str = "\t"; // Tab → open notes (Codex)
const ASK_DELETE: &str = "\u{7f}"; // DEL — clear/skip a Codex row

// ---------------------------------------------------------------------------
// Clear burst (upstream chat spec §7.2) — measured, not derived
// ---------------------------------------------------------------------------

/// Logical line count: `text.split(/\r\n|\r|\n/).length`. Wrapping is
/// irrelevant; one Ctrl+U clears exactly one logical line.
pub fn count_agent_tui_input_lines(text: &str) -> usize {
    let bytes = text.as_bytes();
    let mut lines = 1usize;
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'\r' => {
                lines += 1;
                if bytes.get(index + 1) == Some(&b'\n') {
                    index += 1;
                }
            }
            b'\n' => lines += 1,
            _ => {}
        }
        index += 1;
    }
    lines
}

/// The 2N-1 law: repetitions = 2*lines - 1 of Ctrl+U, then the same count of
/// Ctrl+K. The known line count is a LOWER bound (the user can also type into
/// the TUI directly), so bias upward — overshoot measured perfectly clean on
/// both Claude and Codex; undershoot leaves residue glued onto the next
/// message.
pub fn build_agent_tui_clear_input(line_count: usize) -> String {
    let lines = line_count.clamp(1, AGENT_TUI_CLEAR_MAX_LINES);
    let repetitions = 2 * lines - 1;
    format!(
        "{}{}",
        AGENT_TUI_CLEAR_INPUT_LINE.repeat(repetitions),
        AGENT_TUI_CLEAR_INPUT_FORWARD.repeat(repetitions)
    )
}

pub fn build_agent_tui_clear_input_for_text(text: &str) -> String {
    build_agent_tui_clear_input(count_agent_tui_input_lines(text) + AGENT_TUI_CLEAR_LINE_SLACK)
}

// ---------------------------------------------------------------------------
// Bracketed paste & sanitization (upstream chat spec §7.3/§7.4)
// ---------------------------------------------------------------------------

/// An embedded ESC (e.g. a pasted `\x1b[201~` from scrollback) would close
/// the paste frame early and run the tail as KEYSTROKES; replace with ␛.
pub fn sanitize_bracketed_paste_text(text: &str) -> String {
    text.replace('\u{1b}', "\u{241b}")
}

/// xterm's native paste converts every clipboard newline to CR; direct frames
/// must match, or ConPTY TUIs treat raw LF as submit.
pub fn normalize_terminal_paste_line_endings(text: &str) -> String {
    text.replace("\r\n", "\r").replace('\n', "\r")
}

pub fn wrap_terminal_bracketed_paste_text(text: &str) -> String {
    format!(
        "{BRACKETED_PASTE_START}{}{BRACKETED_PASTE_END}",
        sanitize_bracketed_paste_text(&normalize_terminal_paste_line_endings(text))
    )
}

/// Trailing newline alone counts as multiline.
pub fn is_multiline_draft(text: &str) -> bool {
    text.contains(['\r', '\n'])
}

/// Multiline → framed (NO submit); single-line → sanitized unframed text.
pub fn build_session_chat_paste_bytes(text: &str) -> String {
    if is_multiline_draft(text) {
        wrap_terminal_bracketed_paste_text(text)
    } else {
        sanitize_bracketed_paste_text(text)
    }
}

/// Image paths must LOOK like a real terminal image paste; a plain typed
/// path/@mention is read as text/file-read.
pub fn build_session_chat_image_paste_bytes(path: &str) -> String {
    wrap_terminal_bracketed_paste_text(path)
}

// ---------------------------------------------------------------------------
// Ask-answer keystroke builders (upstream chat spec §8.4/§8.5/§8.6)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AskAnswerKeyGroup {
    /// Written verbatim (arrows, digits, Enter, Tab, DEL).
    Raw(String),
    /// Free text; goes through the paste sanitizer when written.
    Text(String),
}

fn selection_other(selection: Option<&SessionChatQuestionSelection>) -> &str {
    selection
        .and_then(|selection| selection.other.as_deref())
        .unwrap_or_default()
        .trim()
}

fn answer_labels(
    question: &SessionChatQuestion,
    selection: Option<&SessionChatQuestionSelection>,
) -> Vec<String> {
    let mut labels: Vec<String> = selection
        .map(|selection| {
            selection
                .indices
                .iter()
                .map(|index| {
                    question
                        .options
                        .get(*index)
                        .map(|option| option.label.clone())
                        .unwrap_or_default()
                })
                .filter(|label| !label.is_empty())
                .collect()
        })
        .unwrap_or_default();
    let other = selection_other(selection);
    if !other.is_empty() {
        labels.push(other.to_string());
    }
    labels
}

/*
Claude's AskUserQuestion is an arrow-navigate selector: a bare Enter commits
the HIGHLIGHTED default and pasted label text does NOT move the highlight
(bug STA-1860 delivered every non-first pick as the first option). Drive it
by each option's stable 1-based number, which matches the card's badge.
Groups are paced NATIVE_CHAT_QUESTION_STEP_MS apart by the queue because a
navigation keystroke batched with Enter commits before the selector applied
it.
*/
pub fn build_claude_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let multi_question = questions.len() > 1;
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let other = selection_other(selection);
        let type_something = (question.options.len() + 1).to_string(); // the "Type something" row
        let indices: &[usize] = selection
            .map(|selection| selection.indices.as_slice())
            .unwrap_or_default();
        if question.multi_select {
            for index in indices {
                // Each digit TOGGLES a checkbox.
                groups.push(AskAnswerKeyGroup::Raw((index + 1).to_string()));
            }
            if !other.is_empty() {
                groups.push(AskAnswerKeyGroup::Raw(type_something));
                groups.push(AskAnswerKeyGroup::Text(other.to_string()));
                groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            }
            // Multi-select never auto-advances; step to next/Submit tab.
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
        } else if !other.is_empty() {
            // Single-select carries one value, so route ANY answer containing
            // free text through "Type something" as one joined string.
            groups.push(AskAnswerKeyGroup::Raw(type_something));
            groups.push(AskAnswerKeyGroup::Text(
                answer_labels(question, selection).join(", "),
            ));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
        } else if let Some(first) = indices.first() {
            // Selects AND commits; auto-advances in multi-question.
            groups.push(AskAnswerKeyGroup::Raw((first + 1).to_string()));
        } else if multi_question {
            // Unanswered question: step past it.
            groups.push(AskAnswerKeyGroup::Raw(ASK_NEXT_TAB.to_string()));
        }
    }
    let ends_on_submit_tab = multi_question || (questions.len() == 1 && questions[0].multi_select);
    if ends_on_submit_tab && !groups.is_empty() {
        // Final Submit confirmation.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/*
Codex's request_user_input overlay submits on the final option digit,
attaches free text as NOTES to the highlighted row, and starts on the first
row. Notes navigation moves WITHOUT committing via the shortest arrow path.
*/
pub fn build_codex_ask_answer_keys(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> Vec<AskAnswerKeyGroup> {
    let mut groups: Vec<AskAnswerKeyGroup> = Vec::new();
    let mut has_unanswered = false;
    let last_index = questions.len().saturating_sub(1);
    for (question_index, question) in questions.iter().enumerate() {
        let selection = selections.get(question_index);
        let selected_index = selection.and_then(|selection| selection.indices.first().copied());
        let note = selection_other(selection);
        if !note.is_empty() {
            // Default target: the notes row (one past the last option).
            let target_index = selected_index.unwrap_or(question.options.len());
            let row_count = question.options.len() + 1;
            let next_steps = target_index;
            let previous_steps = row_count - target_index;
            let use_previous = previous_steps < next_steps; // pick the shorter path
            let (key, steps) = if use_previous {
                (ASK_PREVIOUS_ROW, previous_steps)
            } else {
                (ASK_NEXT_ROW, next_steps)
            };
            for _ in 0..steps {
                groups.push(AskAnswerKeyGroup::Raw(key.to_string()));
            }
            groups.push(AskAnswerKeyGroup::Raw(ASK_NOTES.to_string()));
            groups.push(AskAnswerKeyGroup::Text(note.to_string()));
            groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
            continue;
        }
        if let Some(selected_index) = selected_index {
            // Digit commits.
            groups.push(AskAnswerKeyGroup::Raw((selected_index + 1).to_string()));
            continue;
        }
        has_unanswered = true;
        groups.push(AskAnswerKeyGroup::Raw(ASK_DELETE.to_string()));
        groups.push(AskAnswerKeyGroup::Raw(if question_index < last_index {
            ASK_NEXT_TAB.to_string()
        } else {
            ASK_ENTER.to_string()
        }));
    }
    if has_unanswered {
        // Codex opens a confirmation; Proceed is highlighted by default.
        groups.push(AskAnswerKeyGroup::Raw(ASK_ENTER.to_string()));
    }
    groups
}

/// Non-stepping agents (Grok): one line per question, IN ORDER; empty answers
/// stay empty lines so N lines === N questions.
pub fn format_ask_answer(
    questions: &[SessionChatQuestion],
    selections: &[SessionChatQuestionSelection],
) -> String {
    questions
        .iter()
        .enumerate()
        .map(|(index, question)| answer_labels(question, selections.get(index)).join(", "))
        .collect::<Vec<_>>()
        .join("\n")
}

pub fn has_ask_answer(selections: &[SessionChatQuestionSelection]) -> bool {
    selections.iter().any(|selection| {
        !selection.indices.is_empty()
            || !selection
                .other
                .as_deref()
                .unwrap_or_default()
                .trim()
                .is_empty()
    })
}

// ---------------------------------------------------------------------------
// Step builders (upstream chat spec §7.5)
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SessionChatSendStep {
    /// Move any existing agent-TUI composer draft into Saved Prompts before
    /// chat takes ownership of the input line. The prompt-editor handshake
    /// answers only after the CLI has durably stashed and cleared that draft.
    PreserveTerminalDraft {
        state_dir: PathBuf,
    },
    /// One `zmx send` stdin burst.
    Write(String),
    SleepMs(u64),
}

/// clear burst → image pastes back-to-back → (300ms settle when text follows
/// images) → paste body → 500ms → SEPARATE Enter.
pub fn build_session_chat_message_steps(
    text: &str,
    image_paths: &[String],
) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    steps.push(SessionChatSendStep::Write(
        build_agent_tui_clear_input_for_text(text),
    ));
    for path in image_paths {
        steps.push(SessionChatSendStep::Write(
            build_session_chat_image_paste_bytes(path),
        ));
    }
    if !text.trim().is_empty() {
        if !image_paths.is_empty() {
            steps.push(SessionChatSendStep::SleepMs(
                SESSION_CHAT_IMAGE_ATTACHMENT_SETTLE_MS,
            ));
        }
        steps.push(SessionChatSendStep::Write(build_session_chat_paste_bytes(
            text,
        )));
    }
    steps.push(SessionChatSendStep::SleepMs(SESSION_CHAT_SUBMIT_DELAY_MS));
    steps.push(SessionChatSendStep::Write(SESSION_CHAT_SUBMIT.to_string()));
    steps
}

/*
Raw key injection: one write, verbatim. No clear burst (there is no input line
to own), no bracketed paste (the bytes ARE keystrokes — framing them would
make the TUI read them as text) and no trailing Enter (the key IS the
submission). Unknown names return None so the handler can reject them instead
of writing something arbitrary.
*/
pub fn build_session_chat_key_steps(key: &str) -> Option<Vec<SessionChatSendStep>> {
    let payload = match key {
        "shift-tab" => SESSION_CHAT_SHIFT_TAB,
        "shift-up" => SESSION_CHAT_SHIFT_UP,
        "shift-down" => SESSION_CHAT_SHIFT_DOWN,
        _ => return None,
    };
    Some(vec![SessionChatSendStep::Write(payload.to_string())])
}

/*
CDXC:SessionChatTerminalPicker 2026-08-22:
Answering an on-screen picker (Claude Code's resume-usage chooser today): type
the chosen row's NUMBER, and nothing else.

This used to walk the highlight with arrow keys and confirm with Enter, which
always answered row 1. Measured on a zmx pty: `ESC [ B` written into that picker
does not move the highlight at all, so every walk was a no-op and the trailing
Enter committed whatever was already highlighted. The digit both selects and
commits — no Enter, no settle, nothing to pace. Same behaviour, same fix, and
same reason as Claude's AskUserQuestion selector above.

One verbatim write: no clear burst and no bracketed paste, because this is a
keystroke for a dialog that owns the input line, not text for a composer.
*/
pub fn build_terminal_picker_answer_steps(answer_key: &str) -> Vec<SessionChatSendStep> {
    vec![SessionChatSendStep::Write(answer_key.to_string())]
}

/// Keystroke groups written 1000ms apart; raw groups go verbatim, text groups
/// through the paste sanitizer.
pub fn build_ask_answer_steps(groups: &[AskAnswerKeyGroup]) -> Vec<SessionChatSendStep> {
    let mut steps = Vec::new();
    for (index, group) in groups.iter().enumerate() {
        if index > 0 {
            steps.push(SessionChatSendStep::SleepMs(SESSION_CHAT_QUESTION_STEP_MS));
        }
        steps.push(SessionChatSendStep::Write(match group {
            AskAnswerKeyGroup::Raw(raw) => raw.clone(),
            AskAnswerKeyGroup::Text(text) => build_session_chat_paste_bytes(text),
        }));
    }
    steps
}

// ---------------------------------------------------------------------------
// Per-session send queue (upstream chat spec §7.6)
// ---------------------------------------------------------------------------

/*
CDXC:SessionChatTerminalNotices 2026-08-19:
Why a send did not complete. The message a caller shows the user is the same as
before; what is new is that the caller can tell "the terminal refused this
message" (the agent CLI in the pane never answered the Ctrl+G handshake, or zmx
would not take the bytes — the crashed-agent case this feature exists to
explain) apart from "this send was never attempted" (superseded by a newer send,
cancelled, or the queue was gone). Only the former is evidence about the
terminal, so only the former may raise a notice.
*/
const SESSION_CHAT_SEND_CANCELLED: &str = "The session chat send was cancelled.";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionChatSendFailure {
    /// Superseded, cancelled, or never dequeued: the terminal was never asked.
    NotAttempted,
    /// The Ctrl+G draft-preservation handshake never completed.
    PreserveTerminalDraft,
    /// A `zmx send` burst was refused.
    Write,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionChatSendError {
    pub failure: SessionChatSendFailure,
    pub message: String,
}

impl SessionChatSendError {
    fn new(failure: SessionChatSendFailure, message: String) -> Self {
        Self { failure, message }
    }

    fn not_attempted(message: String) -> Self {
        Self::new(SessionChatSendFailure::NotAttempted, message)
    }

    /// True when the session's own terminal is what refused the message.
    pub fn terminal_refused(&self) -> bool {
        self.failure != SessionChatSendFailure::NotAttempted
    }
}

struct SessionChatSendJob {
    completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
    project_id: String,
    session_id: String,
    source: &'static str,
    zmx_name: String,
    generation: u64,
    steps: Vec<SessionChatSendStep>,
}

struct SessionChatSendQueue {
    tx: mpsc::UnboundedSender<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
}

static SESSION_CHAT_SEND_QUEUES: OnceLock<Mutex<HashMap<String, SessionChatSendQueue>>> =
    OnceLock::new();

fn queue_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}|{session_id}")
}

/*
Invariant preserved from the upstream chat spec: each sequence owns the input line from its
clear until its Enter fires. One worker task per session drains jobs
serially; a cancelled generation skips queued jobs at dequeue AND aborts the
remaining steps of an in-flight job before its next write/sleep. A failed
zmx write aborts the rest of its sequence so a dangling Enter can never
follow a body that was not delivered. Must be called from within the tokio
runtime (HTTP handlers are).
*/
pub fn enqueue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &'static str,
    steps: Vec<SessionChatSendStep>,
) {
    let _ = queue_session_chat_send(project_id, session_id, zmx_name, source, steps, None);
}

/// Enqueues one sequence on the same per-session worker as fire-and-forget
/// sends, but resolves only after every preservation/write step has completed.
/// Chat message HTTP calls use this so the composer is cleared only after the
/// terminal draft is safe and the new prompt was actually submitted.
pub async fn execute_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &'static str,
    steps: Vec<SessionChatSendStep>,
) -> Result<(), SessionChatSendError> {
    let (completion_tx, completion_rx) = oneshot::channel();
    queue_session_chat_send(
        project_id,
        session_id,
        zmx_name,
        source,
        steps,
        Some(completion_tx),
    )
    .map_err(SessionChatSendError::not_attempted)?;
    completion_rx.await.map_err(|_| {
        SessionChatSendError::not_attempted(
            "The session chat send worker stopped before completing the message.".to_string(),
        )
    })?
}

fn queue_session_chat_send(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &'static str,
    steps: Vec<SessionChatSendStep>,
    completion: Option<oneshot::Sender<Result<(), SessionChatSendError>>>,
) -> Result<(), String> {
    if steps.is_empty() {
        return Ok(());
    }
    let queues = SESSION_CHAT_SEND_QUEUES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut map = queues
        .lock()
        .map_err(|_| "The session chat send queue is unavailable.".to_string())?;
    let queue = map
        .entry(queue_key(project_id, session_id))
        .or_insert_with(|| {
            let (tx, rx) = mpsc::unbounded_channel();
            let generation = Arc::new(AtomicU64::new(0));
            tokio::spawn(run_session_chat_send_worker(rx, generation.clone()));
            SessionChatSendQueue { tx, generation }
        });
    let job = SessionChatSendJob {
        completion,
        project_id: project_id.to_string(),
        session_id: session_id.to_string(),
        source,
        zmx_name: zmx_name.to_string(),
        generation: queue.generation.load(Ordering::SeqCst),
        steps,
    };
    queue
        .tx
        .send(job)
        .map_err(|_| "The session chat send worker is unavailable.".to_string())
}

/// Cancels every queued (and the remaining steps of any in-flight) send for a
/// session by bumping its generation. Later enqueues use the new generation.
pub fn cancel_session_chat_sends(project_id: &str, session_id: &str) {
    let Some(queues) = SESSION_CHAT_SEND_QUEUES.get() else {
        return;
    };
    let Ok(map) = queues.lock() else {
        return;
    };
    if let Some(queue) = map.get(&queue_key(project_id, session_id)) {
        queue.generation.fetch_add(1, Ordering::SeqCst);
    }
}

async fn run_session_chat_send_worker(
    mut rx: mpsc::UnboundedReceiver<SessionChatSendJob>,
    generation: Arc<AtomicU64>,
) {
    while let Some(job) = rx.recv().await {
        let SessionChatSendJob {
            mut completion,
            project_id,
            session_id,
            source,
            zmx_name,
            generation: job_generation,
            steps,
        } = job;
        if job_generation != generation.load(Ordering::SeqCst) {
            if let Some(completion) = completion.take() {
                let _ = completion.send(Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                )));
            }
            continue; // cancelled while queued
        }
        let mut outcome = Ok(());
        for step in steps {
            if job_generation != generation.load(Ordering::SeqCst) {
                outcome = Err(SessionChatSendError::not_attempted(
                    SESSION_CHAT_SEND_CANCELLED.to_string(),
                ));
                break; // cancelled mid-sequence
            }
            match step {
                SessionChatSendStep::SleepMs(delay_ms) => {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                SessionChatSendStep::PreserveTerminalDraft { state_dir } => {
                    if let Err(error) = preserve_terminal_draft(
                        &state_dir,
                        &project_id,
                        &session_id,
                        &zmx_name,
                        &generation,
                        job_generation,
                    )
                    .await
                    {
                        outcome = Err(error);
                        break;
                    }
                }
                SessionChatSendStep::Write(payload) => {
                    if let Err(error) = write_session_chat_payload(
                        &project_id,
                        &session_id,
                        &zmx_name,
                        source,
                        &payload,
                    )
                    .await
                    {
                        outcome = Err(SessionChatSendError::new(
                            SessionChatSendFailure::Write,
                            error,
                        ));
                        break;
                    }
                }
            }
        }
        if let Some(completion) = completion.take() {
            let _ = completion.send(outcome);
        }
    }
}

/// One `zmx send` stdin burst, logged through the shared temporary input log.
async fn write_session_chat_payload(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    source: &'static str,
    payload: &str,
) -> Result<(), String> {
    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatQueueWrite",
        source,
        payload,
    );
    let zmx_name = zmx_name.to_string();
    let payload = payload.to_string();
    let write = tokio::task::spawn_blocking(move || {
        crate::zmx::session_chat_zmx_write(&zmx_name, &payload)
    })
    .await;
    if matches!(write, Ok(Ok(_))) {
        Ok(())
    } else {
        Err("The session terminal did not accept the chat input.".to_string())
    }
}

/// Current terminal text for the session, or `None` when it could not be read
/// whole — a capture whose tail was dropped cannot prove what is on screen.
/// Shared with the send-delivery watchdog (session_chat_watchdog.rs), which
/// takes exactly one of these per timeout event.
pub(crate) async fn capture_session_terminal_text(zmx_name: &str) -> Option<String> {
    let zmx_name = zmx_name.to_string();
    let capture = tokio::task::spawn_blocking(move || {
        crate::zmx::read_zmx_session_screen_capture(&zmx_name)
    })
    .await
    .ok()?
    .ok()?;
    (!capture.truncated).then_some(capture.text)
}

fn prompt_stash_request_path(state_dir: &Path, project_id: &str, session_id: &str) -> PathBuf {
    state_dir
        .join("prompt-stash-requests")
        .join(format!("{project_id}-{session_id}"))
}

fn prompt_handoff_response_path(state_dir: &Path, request_id: &str) -> PathBuf {
    state_dir
        .join("prompt-handoffs")
        .join(format!("{request_id}.json"))
}

/// What the CLI's prompt-editor handshake reported about the composer draft it
/// just moved out of the agent TUI. `prompt_id` is `None` when the composer was
/// empty; `created` marks a stash row this capture owns (as opposed to an
/// update of an existing one), so a caller that only wanted the text can delete
/// it again without destroying a prompt the user had stashed themselves.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CapturedTerminalDraft {
    pub created: bool,
    pub prompt_id: Option<String>,
}

/*
CDXC:SessionChatDraftHandoff 2026-08-18:
Terminal → chat draft transfer, shared by every host. The bytes a user typed
into the agent TUI live only in that TUI's composer, so the sole way to read
them is the Ctrl+G prompt-editor contract: drop a one-shot `handoff:<id>`
marker, send BEL, and let `ghostex prompt-editor` stash the composer into
Saved Prompts, clear it, and answer through the response file. Running it here
rather than in each client is what lets remote gpui sessions and the phone use
it at all — they have no filesystem on the agent's machine.
*/
async fn preserve_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    generation: &AtomicU64,
    job_generation: u64,
) -> Result<(), SessionChatSendError> {
    run_terminal_draft_capture(
        state_dir,
        project_id,
        session_id,
        zmx_name,
        Some((generation, job_generation)),
    )
    .await
    .map(|_| ())
    .map_err(|message| {
        /*
        CDXC:SessionChatTerminalNotices 2026-08-19:
        The generation, not the message text, is what says whether this send was
        superseded while the handshake ran. Anything else here is the CLI in the
        pane failing to answer Ctrl+G at all, which is exactly the evidence the
        terminal-notice escalation acts on.
        */
        if job_generation != generation.load(Ordering::SeqCst) {
            SessionChatSendError::not_attempted(message)
        } else {
            SessionChatSendError::new(SessionChatSendFailure::PreserveTerminalDraft, message)
        }
    })
}

/// Standalone terminal-draft capture for the `/api/handoffSessionChatDraft`
/// endpoint. It deliberately does NOT ride the per-session send queue: the
/// marker's `create_new` open is already the mutual exclusion against a
/// concurrent chat send's preserve step, and a view switch never races a send
/// from the same client.
pub async fn capture_session_chat_terminal_draft(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
) -> Result<CapturedTerminalDraft, String> {
    run_terminal_draft_capture(state_dir, project_id, session_id, zmx_name, None).await
}

async fn run_terminal_draft_capture(
    state_dir: &Path,
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    cancellation: Option<(&AtomicU64, u64)>,
) -> Result<CapturedTerminalDraft, String> {
    let request_id = format!(
        "chat-{}-{}",
        std::process::id(),
        SESSION_CHAT_DRAFT_PRESERVE_SEQUENCE.fetch_add(1, Ordering::Relaxed),
    );
    let marker_path = prompt_stash_request_path(state_dir, project_id, session_id);
    let response_path = prompt_handoff_response_path(state_dir, &request_id);
    let Some(marker_parent) = marker_path.parent() else {
        return Err("The terminal draft stash path is unavailable.".to_string());
    };
    fs::create_dir_all(marker_parent)
        .map_err(|_| "The terminal draft stash path could not be created.".to_string())?;
    if let Some(response_parent) = response_path.parent() {
        fs::create_dir_all(response_parent)
            .map_err(|_| "The terminal draft response path could not be created.".to_string())?;
    }
    let _ = fs::remove_file(&response_path);
    let stale_marker = fs::metadata(&marker_path)
        .ok()
        .and_then(|metadata| metadata.modified().ok())
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|age| age > PROMPT_STASH_REQUEST_FRESHNESS);
    if stale_marker {
        let _ = fs::remove_file(&marker_path);
    }
    let mut marker = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&marker_path)
        .map_err(|_| "The terminal draft is already being moved or stashed.".to_string())?;
    if marker
        .write_all(format!("handoff:{request_id}\n").as_bytes())
        .is_err()
    {
        let _ = fs::remove_file(&marker_path);
        return Err("The terminal draft stash request could not be written.".to_string());
    }

    crate::zmx::log_temporary_zmx_input_write(
        project_id,
        session_id,
        zmx_name,
        "sessionChatPreserveTerminalDraft",
        "session-chat-preserve-draft",
        "\u{7}",
    );
    let zmx_name_owned = zmx_name.to_string();
    let delivered = tokio::task::spawn_blocking(move || {
        crate::zmx::session_chat_zmx_write(&zmx_name_owned, "\u{7}")
    })
    .await;
    if !matches!(delivered, Ok(Ok(_))) {
        let _ = fs::remove_file(&marker_path);
        return Err("The terminal could not start preserving its current draft.".to_string());
    }

    let started = std::time::Instant::now();
    loop {
        let cancelled = cancellation.is_some_and(|(generation, job_generation)| {
            job_generation != generation.load(Ordering::SeqCst)
        });
        if cancelled {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err(SESSION_CHAT_SEND_CANCELLED.to_string());
        }
        if let Ok(text) = fs::read_to_string(&response_path) {
            if let Ok(response) = serde_json::from_str::<serde_json::Value>(&text) {
                let _ = fs::remove_file(&response_path);
                if response.get("ok").and_then(serde_json::Value::as_bool) != Some(true) {
                    return Err("The terminal draft could not be saved.".to_string());
                }
                // An empty composer is a successful capture of nothing: the
                // CLI answers `empty` without touching Saved Prompts.
                if response.get("empty").and_then(serde_json::Value::as_bool) == Some(true) {
                    return Ok(CapturedTerminalDraft::default());
                }
                return Ok(CapturedTerminalDraft {
                    created: response
                        .get("created")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    prompt_id: response
                        .get("promptId")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string),
                });
            }
        }
        if started.elapsed() >= SESSION_CHAT_DRAFT_PRESERVE_TIMEOUT {
            let _ = fs::remove_file(&marker_path);
            let _ = fs::remove_file(&response_path);
            return Err("The terminal did not finish preserving its current draft.".to_string());
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

// ---------------------------------------------------------------------------
// Inline tests: byte builders and keystroke builders are pure and locked here
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session_chat::SessionChatQuestionOption;

    fn question(text: &str, multi_select: bool, options: &[&str]) -> SessionChatQuestion {
        SessionChatQuestion {
            question: text.to_string(),
            header: None,
            multi_select,
            options: options
                .iter()
                .map(|label| SessionChatQuestionOption {
                    label: (*label).to_string(),
                    description: None,
                })
                .collect(),
        }
    }

    fn selection(indices: &[usize], other: Option<&str>) -> SessionChatQuestionSelection {
        SessionChatQuestionSelection {
            indices: indices.to_vec(),
            other: other.map(str::to_string),
        }
    }

    fn raw(value: &str) -> AskAnswerKeyGroup {
        AskAnswerKeyGroup::Raw(value.to_string())
    }

    fn text(value: &str) -> AskAnswerKeyGroup {
        AskAnswerKeyGroup::Text(value.to_string())
    }

    #[test]
    fn clear_burst_follows_the_2n_minus_1_law() {
        // 1 line → 1 repetition.
        assert_eq!(build_agent_tui_clear_input(1), "\u{15}\u{b}");
        // 3 lines → 5 repetitions of each.
        assert_eq!(
            build_agent_tui_clear_input(3),
            format!("{}{}", "\u{15}".repeat(5), "\u{b}".repeat(5))
        );
        // Cap at 40 lines → 79 repetitions; 0 clamps up to 1.
        assert_eq!(
            build_agent_tui_clear_input(1000),
            format!("{}{}", "\u{15}".repeat(79), "\u{b}".repeat(79))
        );
        assert_eq!(build_agent_tui_clear_input(0), "\u{15}\u{b}");
        // For-text adds the 8-line slack: 1 line + 8 → 17 repetitions.
        assert_eq!(
            build_agent_tui_clear_input_for_text("hello"),
            format!("{}{}", "\u{15}".repeat(17), "\u{b}".repeat(17))
        );
        assert_eq!(count_agent_tui_input_lines("a\r\nb\rc\nd"), 4);
        assert_eq!(count_agent_tui_input_lines("plain"), 1);
        assert_eq!(count_agent_tui_input_lines("trailing\n"), 2);
    }

    #[test]
    fn paste_sanitize_and_normalize_match_spec() {
        assert_eq!(
            sanitize_bracketed_paste_text("a\u{1b}[201~b"),
            "a\u{241b}[201~b"
        );
        assert_eq!(
            normalize_terminal_paste_line_endings("a\r\nb\nc"),
            "a\rb\rc"
        );
        // Lone CR is untouched.
        assert_eq!(normalize_terminal_paste_line_endings("a\rb"), "a\rb");
        // Multiline → framed; single line → sanitized unframed.
        assert_eq!(
            build_session_chat_paste_bytes("one\ntwo"),
            "\u{1b}[200~one\rtwo\u{1b}[201~"
        );
        assert_eq!(build_session_chat_paste_bytes("solo"), "solo");
        assert!(is_multiline_draft("text\n"));
        assert!(!is_multiline_draft("text"));
        assert_eq!(
            build_session_chat_image_paste_bytes("/tmp/a.png"),
            "\u{1b}[200~/tmp/a.png\u{1b}[201~"
        );
    }

    #[test]
    fn message_steps_keep_enter_as_separate_delayed_write() {
        let steps = build_session_chat_message_steps("hi", &[]);
        assert_eq!(
            steps,
            vec![
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("hi")),
                SessionChatSendStep::Write("hi".to_string()),
                SessionChatSendStep::SleepMs(500),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        let with_images = build_session_chat_message_steps(
            "what is this",
            &["/tmp/ghostex-paste-1.png".to_string()],
        );
        assert_eq!(
            with_images,
            vec![
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("what is this")),
                SessionChatSendStep::Write(
                    "\u{1b}[200~/tmp/ghostex-paste-1.png\u{1b}[201~".to_string()
                ),
                SessionChatSendStep::SleepMs(300),
                SessionChatSendStep::Write("what is this".to_string()),
                SessionChatSendStep::SleepMs(500),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        // Images without text: no body write, still the delayed Enter.
        let images_only = build_session_chat_message_steps("", &["/tmp/a.png".to_string()]);
        assert_eq!(
            images_only,
            vec![
                SessionChatSendStep::Write(build_agent_tui_clear_input_for_text("")),
                SessionChatSendStep::Write("\u{1b}[200~/tmp/a.png\u{1b}[201~".to_string()),
                SessionChatSendStep::SleepMs(500),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
    }

    #[test]
    fn claude_single_question_single_select_commits_by_digit() {
        let questions = vec![question("Pick one", false, &["A", "B", "C"])];
        let selections = vec![selection(&[1], None)];
        // Digit selects AND commits; single single-select never ends on the
        // Submit tab, so no trailing Enter.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("2")]
        );
    }

    #[test]
    fn claude_free_text_routes_through_type_something() {
        let questions = vec![question("Pick", false, &["A", "B"])];
        let selections = vec![selection(&[0], Some("also this"))];
        // "Type something" is row options.len()+1 = 3; label + other joined.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("3"), text("A, also this"), raw("\r")]
        );
    }

    #[test]
    fn claude_multi_select_toggles_then_advances_then_submits() {
        let questions = vec![question("Pick many", true, &["A", "B", "C"])];
        let selections = vec![selection(&[0, 2], None)];
        // Toggle 1 and 3, step to Submit tab, then the final confirmation
        // (single multiSelect question ends on the Submit tab).
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("1"), raw("3"), raw("\u{1b}[C"), raw("\r")]
        );
    }

    #[test]
    fn claude_multi_question_steps_past_unanswered_and_confirms() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
        ];
        let selections = vec![
            selection(&[0], None),
            SessionChatQuestionSelection::default(),
        ];
        // Q1 digit auto-advances; Q2 unanswered → Right past it; multi-question
        // ends on the Submit tab → final Enter.
        assert_eq!(
            build_claude_ask_answer_keys(&questions, &selections),
            vec![raw("1"), raw("\u{1b}[C"), raw("\r")]
        );
    }

    #[test]
    fn codex_digit_commits_and_notes_use_shortest_arrow_path() {
        let questions = vec![question("Pick", false, &["A", "B", "C"])];
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[2], None)]),
            vec![raw("3")]
        );
        // Note without selection targets the notes row (index 3 of 4 rows):
        // previous_steps = 4-3 = 1 < next_steps = 3 → one Up, Tab, note, Enter.
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[], Some("my note"))]),
            vec![raw("\u{1b}[A"), raw("\t"), text("my note"), raw("\r")]
        );
        // Note attached to row 1 (index 0): zero arrows (already highlighted).
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &[selection(&[0], Some("why"))]),
            vec![raw("\t"), text("why"), raw("\r")]
        );
    }

    #[test]
    fn codex_unanswered_rows_are_skipped_and_confirmed() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
        ];
        let selections = vec![
            SessionChatQuestionSelection::default(),
            selection(&[1], None),
        ];
        // Q1 unanswered → DEL + Right (not last); Q2 digit commits; unanswered
        // remains → trailing Enter for the confirmation dialog.
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &selections),
            vec![raw("\u{7f}"), raw("\u{1b}[C"), raw("2"), raw("\r")]
        );
        // Unanswered LAST question ends its row with Enter instead of Right.
        let tail_unanswered = vec![
            selection(&[0], None),
            SessionChatQuestionSelection::default(),
        ];
        assert_eq!(
            build_codex_ask_answer_keys(&questions, &tail_unanswered),
            vec![raw("1"), raw("\u{7f}"), raw("\r"), raw("\r")]
        );
    }

    #[test]
    fn format_ask_answer_keeps_one_line_per_question() {
        let questions = vec![
            question("First", false, &["A", "B"]),
            question("Second", false, &["X", "Y"]),
            question("Third", false, &["M"]),
        ];
        let selections = vec![
            selection(&[1], None),
            SessionChatQuestionSelection::default(),
            selection(&[0], Some("extra")),
        ];
        assert_eq!(format_ask_answer(&questions, &selections), "B\n\nM, extra");
        assert!(has_ask_answer(&selections));
        assert!(!has_ask_answer(&[SessionChatQuestionSelection::default()]));
    }

    #[test]
    fn key_steps_are_a_single_verbatim_write() {
        assert_eq!(
            build_session_chat_key_steps("shift-tab"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[9;2u".to_string())])
        );
        // No bracketed paste framing, no trailing Enter, no clear burst.
        assert_eq!(build_session_chat_key_steps("shift-tab").unwrap().len(), 1);
        assert_eq!(
            build_session_chat_key_steps("shift-up"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[1;2A".to_string())])
        );
        assert_eq!(
            build_session_chat_key_steps("shift-down"),
            Some(vec![SessionChatSendStep::Write("\u{1b}[1;2B".to_string())])
        );
        assert_eq!(build_session_chat_key_steps("tab"), None);
        assert_eq!(build_session_chat_key_steps(""), None);
    }

    #[test]
    fn ask_answer_steps_space_groups_one_second_apart() {
        let steps = build_ask_answer_steps(&[raw("1"), text("note\nline"), raw("\r")]);
        assert_eq!(
            steps,
            vec![
                SessionChatSendStep::Write("1".to_string()),
                SessionChatSendStep::SleepMs(1_000),
                SessionChatSendStep::Write("\u{1b}[200~note\rline\u{1b}[201~".to_string()),
                SessionChatSendStep::SleepMs(1_000),
                SessionChatSendStep::Write("\r".to_string()),
            ]
        );
        assert!(build_ask_answer_steps(&[]).is_empty());
    }
}
